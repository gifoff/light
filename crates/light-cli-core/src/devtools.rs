//! ТЗ 0.16.0: клиент DevTools по TCP для `light devtools <cmd>`.
//!
//! Подключается к `127.0.0.1:<port>` (по умолчанию 9229), отправляет одну
//! команду в текстовом виде (line-delimited, `\n`) и читает ответ до
//! следующего `\n`. Если порт не принимает соединения — ошибка «Приложение не
//! запущено».

use std::io::Write;
use std::net::TcpStream;
use std::time::Duration;

/// Порт DevTools по умолчанию (совпадает с `DEVTOOLS_PORT` в [`crate::dev`]).
pub const DEFAULT_PORT: u16 = 9229;

/// Ошибка клиента DevTools.
#[derive(Debug, thiserror::Error)]
pub enum DevtoolsError {
    /// Приложение не запущено (порт не отвечает).
    #[error("Приложение не запущено. Запустите light dev")]
    NotRunning,
    /// IO-ошибка (таймаут, разрыв соединения, …).
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    /// Неверные аргументы команды (например, пустой key).
    #[error("{0}")]
    BadArgs(String),
}

/// Результат операции клиента.
pub type Result<T> = std::io::Result<T>;

/// Аргументы одной команды `light devtools`.
#[derive(Debug, Clone)]
pub struct DevtoolsRequest {
    /// Имя подкоманды: tree, layout, find, click, type, key, state, shot,
    /// session.
    pub cmd: String,
    /// Строковое тело запроса после имени команды (может быть пустым).
    pub body: Option<String>,
    /// Путь для сохранения скриншота (для `shot --path`).
    pub save_path: Option<std::path::PathBuf>,
    /// Разрешён ли вывод JSON (`--json`).
    pub json: bool,
}

impl DevtoolsRequest {
    /// Строит текст команды для отправки на сервер. Имя команды передаётся в
    /// верхнем регистре (протокол DevTools: TREE, LAYOUT, FIND, CLICK, …),
    /// а тело запроса — как есть (JSON может содержать пробелы).
    pub fn to_wire(&self) -> String {
        let name = self.cmd.to_ascii_uppercase();
        // Специальный случай: CLI-подкоманда `state` → серверная SNAPSHOT.
        let name = if name == "STATE" {
            "SNAPSHOT".to_owned()
        } else {
            name
        };
        match &self.body {
            Some(b) => format!("{name} {b}"),
            None => name,
        }
    }
}

/// Отправляет запрос и возвращает сырую строку ответа (без завершающего `\n`).
///
/// При невозможности подключиться возвращает [`DevtoolsError::NotRunning`].
pub fn send_request(
    port: u16,
    req: &DevtoolsRequest,
) -> std::result::Result<String, DevtoolsError> {
    let addr = format!("127.0.0.1:{port}");
    let socket: std::net::SocketAddr = addr.parse().map_err(|_| DevtoolsError::NotRunning)?;
    let stream = TcpStream::connect_timeout(&socket, Duration::from_secs(3))
        .map_err(|_| DevtoolsError::NotRunning)?;
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
    let mut buf_reader = std::io::BufReader::new(stream.try_clone()?);
    let mut writer = stream;
    let wire = req.to_wire();
    writer.write_all(wire.as_bytes())?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    let mut line = String::new();
    // Ответ может быть длинным (base64 PNG) — читаем до `\n` без ограничения
    // в 8 КБ ( BufRead::read_line умеет большие строки, но используем read_until
    // вручную, чтобы не зависеть от лимита read_line).
    use std::io::Read;
    let mut byte = [0u8; 1];
    loop {
        if buf_reader.read(&mut byte[..1])? == 0 {
            break;
        }
        if byte[0] == b'\n' {
            break;
        }
        if byte[0] == b'\r' {
            continue;
        }
        line.push(byte[0] as char);
    }
    Ok(line)
}

/// Декодирует base64-PNG (ответ `base64:<data>`) и сохраняет его в файл.
/// Возвращает путь к сохранённому файлу.
pub fn save_png(
    base64_payload: &str,
    path: &std::path::Path,
) -> std::io::Result<std::path::PathBuf> {
    use base64::Engine;
    let b64 = base64_payload
        .strip_prefix("base64:")
        .unwrap_or(base64_payload);
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|e| std::io::Error::other(format!("base64: {e}")))?;
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, &bytes)?;
    Ok(path.to_path_buf())
}

/// Разбирает флаг `--attr k=v` в пару (ключ, значение).
pub fn parse_attr(s: &str) -> std::result::Result<(String, String), DevtoolsError> {
    let (k, v) = s
        .split_once('=')
        .ok_or_else(|| DevtoolsError::BadArgs(format!("--attr expects k=v, got `{s}`")))?;
    Ok((k.to_owned(), v.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(cmd: &str, body: Option<&str>) -> DevtoolsRequest {
        DevtoolsRequest {
            cmd: cmd.to_owned(),
            body: body.map(|s| s.to_owned()),
            save_path: None,
            json: false,
        }
    }

    /// Простая команда без тела → верхний регистр имени.
    #[test]
    fn to_wire_simple() {
        assert_eq!(req("tree", None).to_wire(), "TREE");
        assert_eq!(req("layout", None).to_wire(), "LAYOUT");
        assert_eq!(req("shot", None).to_wire(), "SHOT");
    }

    /// Тело передаётся как есть (JSON с пробелами не ломается).
    #[test]
    fn to_wire_with_body() {
        assert_eq!(
            req("find", Some(r#"{"tag":"button","text":"+1"}"#)).to_wire(),
            "FIND {\"tag\":\"button\",\"text\":\"+1\"}"
        );
        // TYPE: текст с пробелом сохраняется полностью.
        assert_eq!(
            req("type", Some("0.1 hello world")).to_wire(),
            "TYPE 0.1 hello world"
        );
        assert_eq!(req("key", Some("0.1 Enter")).to_wire(), "KEY 0.1 Enter");
        assert_eq!(req("click", Some("0.0")).to_wire(), "CLICK 0.0");
    }

    /// CLI-подкоманда `state` маппится на серверную SNAPSHOT.
    #[test]
    fn to_wire_state_is_snapshot() {
        assert_eq!(req("state", None).to_wire(), "SNAPSHOT");
    }

    /// `--attr k=v` разбирается; без `=` — ошибка.
    #[test]
    fn parse_attr_roundtrip() {
        assert_eq!(
            parse_attr("class=btn").expect("k=v"),
            ("class".to_owned(), "btn".to_owned())
        );
        assert!(parse_attr("no-equals").is_err());
    }
}
