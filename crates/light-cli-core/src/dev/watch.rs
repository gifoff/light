use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

/// Определяет корневой каталог пакета по имени (через cargo metadata).
pub fn resolve_package_root(package: Option<&str>, cwd: &Path) -> PathBuf {
    let Some(name) = package else {
        return cwd.to_path_buf();
    };
    let output = Command::new("cargo")
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(cwd)
        .output();
    let Ok(output) = output else {
        eprintln!("light dev: `cargo metadata` не запустился — слежу за cwd");
        return cwd.to_path_buf();
    };
    if !output.status.success() {
        eprintln!(
            "light dev: `cargo metadata` завершился с ошибкой — слежу за cwd: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        return cwd.to_path_buf();
    }
    let Ok(meta) = serde_json::from_slice::<serde_json::Value>(&output.stdout) else {
        eprintln!("light dev: не удалось разобрать вывод `cargo metadata` — слежу за cwd");
        return cwd.to_path_buf();
    };
    let Some(packages) = meta.get("packages").and_then(|p| p.as_array()) else {
        eprintln!("light dev: в выводе `cargo metadata` нет полей `packages` — слежу за cwd");
        return cwd.to_path_buf();
    };
    let found = packages
        .iter()
        .find(|pkg| pkg.get("name").and_then(|n| n.as_str()) == Some(name));
    match found
        .and_then(|pkg| pkg.get("manifest_path"))
        .and_then(|m| m.as_str())
    {
        Some(manifest_path) => PathBuf::from(manifest_path)
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| cwd.to_path_buf()),
        None => {
            eprintln!("light dev: пакет `{name}` не найден в воркспейсе — слежу за cwd");
            cwd.to_path_buf()
        }
    }
}

/// Определяет корень воркспейса через `cargo metadata --no-deps`.
///
/// Возвращает `workspace_root` из JSON-вывода. Для одиночного крейта (без
/// workspace) поле всё равно есть — `workspace_root == package_root`.
/// Если `cargo metadata` недоступен или вывод не парсится — `None` (fallback
/// на `package_root` вызывающим).
pub fn resolve_workspace_root(cwd: &Path) -> Option<PathBuf> {
    let output = Command::new("cargo")
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(cwd)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let meta: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    meta.get("workspace_root")
        .and_then(|r| r.as_str())
        .map(PathBuf::from)
}

/// Определяет имя пакета, в котором находится пользователь: поднимается вверх
/// от `cwd`, ищет ближайший `Cargo.toml` с `[package].name`; если не найден —
/// использует `cargo metadata --no-deps` (одиночный пакет → его имя,
/// workspace-член → член, чей `manifest_path` ближе всего к `cwd`).
///
/// Возвращает `None`, если пакет определить невозможно.
pub fn detect_package_from_cwd(cwd: &Path) -> Option<String> {
    // Шаг 1: ближайший Cargo.toml вверх по дереву каталогов.
    let mut dir = cwd.to_path_buf();
    loop {
        let manifest = dir.join("Cargo.toml");
        if manifest.is_file()
            && let Ok(text) = std::fs::read_to_string(&manifest)
            && let Some(name) = parse_package_name(&text)
        {
            return Some(name);
        }
        if !dir.pop() {
            break;
        }
    }

    // Шаг 2: `cargo metadata --no-deps` в cwd.
    let output = Command::new("cargo")
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(cwd)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let meta: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    let packages = meta.get("packages").and_then(|p| p.as_array())?;
    if packages.len() == 1 {
        return packages[0]
            .get("name")
            .and_then(|n| n.as_str())
            .map(String::from);
    }
    // Workspace: выбираем пакет, чей manifest_path ближе всего к cwd
    // (больше общих компонентов нормализованного пути — ближе).
    let cwd_str = cwd.to_string_lossy().replace('\\', "/");
    let mut best: Option<(usize, String)> = None;
    for pkg in packages {
        let manifest = pkg.get("manifest_path").and_then(|m| m.as_str())?;
        let manifest_norm = manifest.replace('\\', "/");
        let common = common_path_len(&cwd_str, &manifest_norm);
        let name = pkg
            .get("name")
            .and_then(|n| n.as_str())
            .unwrap_or_default()
            .to_string();
        if best.as_ref().is_none_or(|(d, _)| *d <= common) {
            best = Some((common, name));
        }
    }
    best.map(|(_, name)| name).filter(|n| !n.is_empty())
}

/// Извлекает `[package].name` из текста `Cargo.toml` (примитивный парсер:
/// ищет секцию `[package]` и первое `name = "..."` в ней).
pub(super) fn parse_package_name(toml_text: &str) -> Option<String> {
    let mut in_package = false;
    for line in toml_text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_package = trimmed == "[package]";
            if !in_package {
                continue;
            }
        }
        if in_package && trimmed.starts_with("name") {
            let rest = trimmed.split_once('=')?.1.trim();
            let rest = rest.strip_prefix('"')?.strip_suffix('"')?;
            if !rest.is_empty() {
                return Some(rest.to_string());
            }
        }
    }
    None
}

/// Длина общего префикса двух нормализованных (slash-разделителей) путей,
/// по числу компонентов.
pub(super) fn common_path_len(a: &str, b: &str) -> usize {
    let a_parts: Vec<&str> = a.split('/').collect();
    let b_parts: Vec<&str> = b.split('/').collect();
    a_parts
        .iter()
        .zip(b_parts.iter())
        .take_while(|(x, y)| x == y)
        .count()
}

/// Строит список путей для наблюдения: `src/` и `templates/` (только если
/// существуют) плюс `Cargo.toml` (если есть).
pub fn watch_paths(root: &std::path::Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for dir in ["src", "templates"] {
        let p = root.join(dir);
        if p.is_dir() {
            paths.push(p);
        }
    }
    let manifest = root.join("Cargo.toml");
    if manifest.exists() {
        paths.push(manifest);
    }
    paths
}

/// Путь файла состояния относительно `root` (по умолчанию `.light/state.json`;
/// переопределяется переменной окружения `LIGHT_STATE_FILE`).
pub fn state_file_path(root: &std::path::Path) -> PathBuf {
    match std::env::var_os("LIGHT_STATE_FILE") {
        Some(var) if !var.is_empty() => PathBuf::from(var),
        _ => root.join(".light").join("state.json"),
    }
}

/// Устанавливает `LIGHT_STATE_FILE` на `Command`, указывая на абсолютный путь
/// `root/.light/state.json` (или значение переменной окружения, если задана).
/// Без этого ребёнок читает относительный `.light/state.json` от своего CWD,
/// который может не совпадать с корнем проекта.
pub(super) fn apply_state_env(cmd: &mut std::process::Command, root: &std::path::Path) {
    let path = state_file_path(root);
    eprintln!("light dev: state file for child: {:?}", path);
    cmd.env("LIGHT_STATE_FILE", &path);
}

/// Устанавливает `LIGHT_WINDOW_FILE` на `Command`, указывая на абсолютный путь
/// `root/.light/window.json` (или значение переменной окружения, если задана).
/// Без этого ребёнок читает относительный `.light/window.json` от своего CWD,
/// который может не совпадать с корнем пакета → supervisor и child работают
/// с разными файлами.
pub(super) fn apply_window_env(cmd: &mut std::process::Command, root: &std::path::Path) {
    let path = window_file_path(root);
    eprintln!("light dev: window file for child: {:?}", path);
    cmd.env("LIGHT_WINDOW_FILE", &path);
}

/// Путь файла геометрии окна относительно `root` (по умолчанию
/// `.light/window.json`; переопределяется `LIGHT_WINDOW_FILE`). Совпадает с
/// путём, который читает/пишет окно приложения (`light-fw-core::window`).
pub fn window_file_path(root: &std::path::Path) -> PathBuf {
    match std::env::var_os("LIGHT_WINDOW_FILE") {
        Some(var) if !var.is_empty() => PathBuf::from(var),
        _ => root.join(".light").join("window.json"),
    }
}

/// Отправляет команду `SAVE_STATE\n` в TCP-сервер DevTools на
/// `127.0.0.1:<port>` и закрывает соединение. Ошибки (сервер не запущен,
/// порт занят) не являются фатальными — просто проигнорируются.
pub fn request_save_state(port: u16) -> std::io::Result<()> {
    use std::io::Write;
    use std::net::TcpStream;
    let mut stream = TcpStream::connect(("127.0.0.1", port))?;
    stream.write_all(b"SAVE_STATE\n")?;
    Ok(())
}

/// Отправляет команду `RELOAD_TEMPLATE\n` в TCP-сервер DevTools на
/// `127.0.0.1:<port>` и закрывает соединение. Приложение перечитывает
/// HTML-шаблон с диска и перерисовывает UI без пересборки. Ошибки не
/// являются фатальными — просто проигнорируются.
pub fn request_reload_template(port: u16) -> std::io::Result<()> {
    use std::io::Write;
    use std::net::TcpStream;
    let mut stream = TcpStream::connect(("127.0.0.1", port))?;
    stream.write_all(b"RELOAD_TEMPLATE\n")?;
    Ok(())
}

/// ТЗ 0.15.1-fix3: graceful save перед swap-kill.
///
/// Отправляет `SAVE_ALL\n` в TCP-сервер DevTools на `127.0.0.1:<port>`
/// и **ожидаает** ответа `ok` от UI-потока (до 500 мс). UI-поток синхронно
/// сохраняет `state.json` + принудительно `window.json` (без дебаунса)
/// и только затем отвечает. Если таймаут или ошибка — логим и продолжаем
/// (fallback: TerminateProcess как раньше, debounce-запись может не успеть).
pub(super) fn request_save_all(port: u16) {
    use std::io::BufRead;
    use std::net::TcpStream;

    let result = (|| -> Result<(), String> {
        let stream = TcpStream::connect(("127.0.0.1", port)).map_err(|e| e.to_string())?;
        stream
            .set_read_timeout(Some(Duration::from_millis(500)))
            .map_err(|e| e.to_string())?;
        stream
            .set_write_timeout(Some(Duration::from_millis(500)))
            .map_err(|e| e.to_string())?;
        {
            use std::io::Write as _;
            let mut w = &stream;
            w.write_all(b"SAVE_ALL\n").map_err(|e| e.to_string())?;
        }
        let mut reader = std::io::BufReader::new(&stream);
        let mut line = String::new();
        let bytes_read = reader.read_line(&mut line).map_err(|e| e.to_string())?;
        if bytes_read == 0 {
            return Err("connection closed before response".to_owned());
        }
        let reply = line.trim();
        if reply != "ok" {
            return Err(format!("unexpected reply: {reply}"));
        }
        Ok(())
    })();
    match result {
        Ok(()) => {
            eprintln!("[SAVE_ALL] sent by supervisor, replied ok");
        }
        Err(e) => {
            eprintln!(
                "light dev: SAVE_ALL timeout/error: {e} \
                 (falling back to TerminateProcess without graceful save)"
            );
        }
    }
}

/// Проверяет, открыт ли TCP-порт DevTools (на нём приложение принимает команду
/// `SAVE_STATE`). `TcpStream::connect` к свободному порту возвращает ошибку
/// мгновенно (connection refused), так что это дешёвая проверка.
pub fn is_port_open(port: u16) -> bool {
    use std::net::TcpStream;
    TcpStream::connect(("127.0.0.1", port)).is_ok()
}
