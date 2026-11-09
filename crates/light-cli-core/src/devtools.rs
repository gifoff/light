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

/// Путь к целевому PNG: если указан только каталог (существующая директория
/// или имя без расширения), добавляем `<ts>.png`; иначе путь как есть.
fn resolve_png_path(path: &std::path::Path) -> std::path::PathBuf {
    if path.is_dir() {
        let ts = format_unix_now();
        path.join(format!("{ts}.png"))
    } else if path.extension().is_none() {
        // Имя без расширения → считаем каталогом: `<name>_<ts>.png` внутри.
        // Для `.` / `` имя заменяем на `screenshot`.
        let base = path
            .file_name()
            .and_then(|n| n.to_str())
            .filter(|n| !n.is_empty() && *n != ".")
            .map(String::from)
            .unwrap_or_else(|| "screenshot".to_owned());
        path.join(format!("{base}_{}.png", format_unix_now()))
    } else {
        path.to_path_buf()
    }
}

/// Локальная метка времени для имени файла (YYYYMMDD_HHMMSS), без зависимостей.
fn format_unix_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = (secs / 86400) as i64;
    let sod = (secs % 86400) as u32;
    let (y, mo, day) = civil_from_days_local(days);
    format!(
        "{y:04}{mo:02}{day:02}_{:02}{:02}{:02}",
        sod / 3600,
        (sod % 3600) / 60,
        sod % 60
    )
}

/// Алгоритм «civil from days» (Howard Hinnant) — копия из session.rs, чтобы
/// не тянуть приватную функцию.
fn civil_from_days_local(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if mo <= 2 { y + 1 } else { y };
    (y, mo, d)
}

/// Проверка ответа сервера: если он НЕ похож на скриншот (`base64:`), CLI
/// обязан вернуть читаемую ошибку, а не падать с `Invalid symbol` при
/// декодировании (баг 3 ТЗ 0.16.1).
pub fn is_screenshot_response(raw: &str) -> bool {
    raw.starts_with("base64:")
}

/// Сообщение об ошибке, когда сервер не вернул изображение (узнаётся по
/// строкам `unknown command`, `error: …`, `timeout` и т.п.).
pub fn shot_error_message(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed == "unknown command" {
        "Сервер не поддерживает команду SHOT (возможно, устаревшая версия приложения)".to_owned()
    } else if trimmed.starts_with("error:") || trimmed == "timeout" {
        format!("Ошибка от сервера DevTools: {trimmed}")
    } else {
        format!("Ожидался ответ `base64:<png>`, получен: `{trimmed}`")
    }
}

/// Декодирует base64-PNG (ответ `base64:<data>`) и сохраняет его в файл.
/// Возвращает путь к сохранённому файлу.
///
/// Баг 2 (ТЗ 0.16.1): если `--path` указывает на существующий каталог (или
/// имя без расширения), автоматически дописываем `_<ts>.png`. Родительский
/// каталог создаётся, если отсутствует. Ошибка `os error 5` больше не
/// возникает, если папка существует и доступна на запись.
pub fn save_png(
    base64_payload: &str,
    path: &std::path::Path,
) -> std::io::Result<std::path::PathBuf> {
    use base64::Engine;
    if !is_screenshot_response(base64_payload) {
        return Err(std::io::Error::other(shot_error_message(base64_payload)));
    }
    let target = resolve_png_path(path);
    let b64 = base64_payload
        .strip_prefix("base64:")
        .unwrap_or(base64_payload);
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|e| std::io::Error::other(format!("base64: {e}")))?;
    if let Some(parent) = target.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&target, &bytes)?;
    Ok(target)
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

    /// ТЗ 0.16.1, баг 3: ответ сервера «не скриншот» детектируется.
    #[test]
    fn shot_error_detection() {
        assert!(is_screenshot_response("base64:AAAA"));
        assert!(!is_screenshot_response("unknown command"));
        assert!(!is_screenshot_response("timeout"));
        assert!(!is_screenshot_response("error: no frame"));
        let m = shot_error_message("unknown command");
        assert!(m.contains("SHOT"), "{m}");
        assert!(m.contains("устаревша"), "{m}");
    }

    /// ТЗ 0.16.1, баг 2: путь-каталог получает имя `<ts>.png`.
    #[test]
    fn resolve_png_path_dir_gets_timestamped_name() {
        let base = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        let dir = base.join(format!("_resolve_dir_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create");
        let p = resolve_png_path(&dir);
        assert!(p.starts_with(&dir), "{}", p.display());
        assert!(
            p.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .ends_with(".png")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// ТЗ 0.16.1, баг 2: имя без расширения → `<name>_<ts>.png` внутри.
    #[test]
    fn resolve_png_path_no_extension() {
        let p = resolve_png_path(std::path::Path::new("shots/shots"));
        assert!(
            p.starts_with(std::path::Path::new("shots/shots")),
            "{}",
            p.display()
        );
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        assert!(name.ends_with(".png"), "{name}");
        assert!(name.starts_with("shots_"), "{name}");
    }

    /// ТЗ 0.16.1, баг 2: путь с расширением `.png` возвращается как есть.
    #[test]
    fn resolve_png_path_with_png_ext() {
        let p = resolve_png_path(std::path::Path::new("out/frame.png"));
        assert_eq!(p, std::path::PathBuf::from("out/frame.png"));
    }

    /// ТЗ 0.16.1, баг 2 + 3: `save_png` в каталог создаёт файл и не падает;
    /// неизвестная команда → читаемая ошибка, не `Invalid symbol`.
    #[test]
    fn save_png_writes_file_and_rejects_unknown_command() {
        use base64::Engine;
        // Уникальный каталог рядом с исполняемым файлом — обходим `C:\Temp`
        // (в этой среде на него нет прав записи, os error 5).
        let dir = std::env::current_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("."))
            .join(format!("_save_png_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create dir");
        // Ложные "PNG" данные (не настоящий PNG) — проверяется только round-trip.
        let png_bytes: Vec<u8> = vec![137, 80, 78, 71, 13, 10, 26, 10, 1, 2, 3, 4];
        let b64 = format!(
            "base64:{}",
            base64::engine::general_purpose::STANDARD.encode(&png_bytes)
        );
        let p = save_png(&b64, &dir).expect("save");
        assert!(p.is_file(), "{} должен существовать", p.display());
        assert_eq!(std::fs::read(&p).unwrap(), png_bytes);
        // Баг 3: неизвестная команда → читаемая ошибка, не Invalid symbol.
        let err = save_png("unknown command", &dir).expect_err("должен быть error");
        assert!(err.to_string().contains("SHOT"), "{}", err);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
