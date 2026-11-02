//! Горячая перезагрузка приложения Light: супервизор `light dev`.
//!
//! Запускает приложение (`cargo run --features devtools`) как дочерний процесс,
//! следит за исходниками (`src/`, `templates/`, `Cargo.toml`), а при изменении
//! — сохраняет состояние приложения (команда `SAVE_STATE` через TCP-сервер
//! DevTools на порту 9229), убивает процесс и перезапускает его. При следующем
//! запуске приложение восстанавливает состояние из `.light/state.json`
//! (см. модуль `devtools` в `light-fw-core`).
//!
//! Надёжный мониторинг файлов использует crate `notify` (RecursionMode::Normal);
//! отложенный (debounce ~150 мс) повторный запуск координирует отдельный
//! поток, чтобы кратковременные серии событий не запускали много перезагрузок.

#[cfg(unix)]
use std::os::unix::process::ExitStatusExt as _;
#[cfg(windows)]
use std::os::windows::process::ExitStatusExt as _;
use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use notify::Watcher as _;

const DEBOUNCE: Duration = Duration::from_millis(150);
const DEVTOOLS_PORT: u16 = 9229;

/// Разобранные параметры команды `light dev`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DevArgs {
    /// Пакет воркспейса, который запускается (`cargo run -p <package>`).
    pub package: Option<String>,
    /// Дополнительные флаги cargo, передаваемые после имени пакета
    /// (например, `--release`).
    pub extra_cargo_args: Vec<String>,
}

/// Чистый разбор сырых аргументов командной строки в [`DevArgs`].
///
/// Правила: первый аргумент (если он существует и не начинается с `-`)
/// считается именем пакета; все последующие аргументы попадают в
/// `extra_cargo_args`. Это изолировано от `clap`, чтобы тестировать парсинг
/// без запуска бинарника.
///
/// # Примеры
///
/// ```
/// use light_fw_cli_core::dev::parse_dev_command;
///
/// // `light dev example-devtools-demo --release`
/// let args = parse_dev_command(&["example-devtools-demo".into(), "--release".into()]);
/// assert_eq!(args.package.as_deref(), Some("example-devtools-demo"));
/// assert_eq!(args.extra_cargo_args, vec!["--release"]);
///
/// // `light dev` (без имени пакета)
/// let empty: &[String] = &[];
/// let args = parse_dev_command(empty);
/// assert_eq!(args.package, None);
/// assert!(args.extra_cargo_args.is_empty());
/// ```
pub fn parse_dev_command(raw: &[String]) -> DevArgs {
    let mut out = DevArgs::default();
    for (i, arg) in raw.iter().enumerate() {
        if i == 0 && !arg.starts_with('-') {
            out.package = Some(arg.clone());
        } else {
            out.extra_cargo_args.push(arg.clone());
        }
    }
    out
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

/// Убивает дочерний процесс и дожидается его завершения.
pub fn kill_child(child: &mut Child) {
    if let Err(e) = child.kill() {
        eprintln!("light dev: не удалось завершить дочерний процесс: {e}");
    }
    let _ = child.wait();
}

/// Точка входа команды `light dev`: запускает дочерний процесс и цикл
/// наблюдения. Возвращает код завершения приложения (не `anyhow`, чтобы ядро
/// CLI не тягло зависимость, которой нет в его манифесте).
///
/// Не вызывается автоматически из других частей библиотеки — только из
/// бинарного `light-fw-cli` (см. `cmd_dev` в `light-cli`).
pub fn run(args: &DevArgs) -> std::result::Result<i32, std::io::Error> {
    let cwd = std::env::current_dir()?;
    let spawn_cmd = |args: &DevArgs, cwd: &std::path::Path| -> Command {
        let mut cmd = Command::new("cargo");
        cmd.arg("run");
        if let Some(package) = &args.package {
            cmd.args(["-p", package]);
        }
        cmd.args(["--features", "devtools"]);
        cmd.args(&args.extra_cargo_args);
        cmd.stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .current_dir(cwd);
        cmd
    };

    eprintln!(
        "light dev: запускаю `cargo run{} --features devtools {}` (cwd: {})",
        args.package
            .as_ref()
            .map(|p| format!(" -p {p}"))
            .unwrap_or_default(),
        args.extra_cargo_args.join(" "),
        cwd.display()
    );

    let mut child = spawn_cmd(args, &cwd).spawn()?;

    // Канал на перезапуск от наблюдателя.
    let (restart_tx, restart_rx) = mpsc::channel::<Instant>();
    // Сигнал завершиться наблюдателю.
    let (done_tx, done_rx) = mpsc::channel::<()>();

    let watch_paths_copy = watch_paths(&cwd);
    let watcher = thread::spawn(move || {
        if watch_paths_copy.is_empty() {
            eprintln!("light dev: нет путей для наблюдения (нет src/, templates/, Cargo.toml)");
            return;
        }
        let (tx, rx) = mpsc::channel::<notify::Result<notify::Event>>();
        let mut watcher = match notify::recommended_watcher(move |res| {
            let _ = tx.send(res);
        }) {
            Ok(w) => w,
            Err(e) => {
                eprintln!("light dev: не удалось создать файловый наблюдатель: {e}");
                return;
            }
        };
        for path in &watch_paths_copy {
            if let Err(e) = watcher.watch(path, notify::RecursiveMode::NonRecursive) {
                eprintln!("light dev: не удалось следить за {}: {e}", path.display());
            }
        }
        let mut last_event: Option<Instant> = None;
        loop {
            if done_rx.try_recv().is_ok() {
                break;
            }
            match rx.recv_timeout(Duration::from_millis(50)) {
                Ok(Ok(event)) => {
                    // Игнорируем события вне watched-путей.
                    let relevant = event
                        .paths
                        .iter()
                        .any(|p| watch_paths_copy.iter().any(|w| p.starts_with(w)));
                    if relevant {
                        last_event = Some(Instant::now());
                    }
                }
                Ok(Err(e)) => {
                    eprintln!("light dev: ошибка наблюдателя: {e}");
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if let Some(t) = last_event
                        && t.elapsed() >= DEBOUNCE
                    {
                        last_event = None;
                        let _ = restart_tx.send(Instant::now());
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
    });

    loop {
        // Внутренний цикл опроса: выходим в наружный цикл по первому событию
        // (перезапуск от наблюдателя либо завершение дочернего процесса).
        enum Ev {
            Restart,
            Exited(std::process::ExitStatus),
        }
        let ev = loop {
            if restart_rx.try_recv().is_ok() {
                break Ev::Restart;
            }
            match child.try_wait() {
                Ok(Some(s)) => break Ev::Exited(s),
                Ok(None) => {}
                Err(e) => {
                    eprintln!("light dev: ошибка ожидания дочернего процесса: {e}");
                    break Ev::Exited(ExitStatus::from_raw(1));
                }
            }
            thread::sleep(Duration::from_millis(50));
        };
        match ev {
            Ev::Exited(status) => {
                let _ = done_tx.send(());
                let _ = watcher.join();
                let code = status.code().unwrap_or(1);
                eprintln!("light dev: приложение завершилось с кодом {code}");
                return Ok(code);
            }
            Ev::Restart => {
                eprintln!("light dev: обнаружены изменения — сохраняю состояние и перезапускаю…");
                if let Err(e) = request_save_state(DEVTOOLS_PORT) {
                    eprintln!(
                        "light dev: не удалось отправить SAVE_STATE: {e} \
                         (состояние может быть не сохранено)"
                    );
                }
                // Короткая пауза: даём UI-потоку время обработать команду и
                // записать файл, прежде чем уберём процесс.
                thread::sleep(Duration::from_millis(200));
                kill_child(&mut child);
                match spawn_cmd(args, &cwd).spawn() {
                    Ok(c) => {
                        child = c;
                        eprintln!("light dev: приложение перезапущено");
                    }
                    Err(e) => {
                        eprintln!("light dev: не удалось перезапустить приложение: {e}");
                        let _ = done_tx.send(());
                        let _ = watcher.join();
                        return Ok(1);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::parse_dev_command;

    /// Без аргументов: ни пакета, ни доп. флагов.
    #[test]
    fn parse_empty() {
        let args = parse_dev_command(&[]);
        assert!(args.package.is_none());
        assert!(args.extra_cargo_args.is_empty());
    }

    /// Один аргумент — имя пакета, без доп. флагов.
    #[test]
    fn parse_package_only() {
        let args = parse_dev_command(&["my-app".into()]);
        assert_eq!(args.package.as_deref(), Some("my-app"));
        assert!(args.extra_cargo_args.is_empty());
    }

    /// Пакет + доп. флаги: первый свободный — пакет, остальное — в cargo.
    #[test]
    fn parse_package_and_extra() {
        let args = parse_dev_command(&[
            "my-app".into(),
            "--release".into(),
            "--".into(),
            "arg0".into(),
        ]);
        assert_eq!(args.package.as_deref(), Some("my-app"));
        assert_eq!(
            args.extra_cargo_args,
            vec!["--release".to_owned(), "--".to_owned(), "arg0".to_owned()]
        );
    }

    /// Несколько позиционных: первый — пакет, остальные — доп. флаги.
    #[test]
    fn parse_multiple_positionals_first_is_package() {
        let args = parse_dev_command(&["crate-a".into(), "crate-b".into(), "flag".into()]);
        assert_eq!(args.package.as_deref(), Some("crate-a"));
        assert_eq!(
            args.extra_cargo_args,
            vec!["crate-b".to_owned(), "flag".to_owned()]
        );
    }

    /// Только доп. флаги (начинаются с `-`): пакета нет, всё уходит в cargo.
    #[test]
    fn parse_flags_only_no_package() {
        let args = parse_dev_command(&["--release".into(), "--quiet".into()]);
        assert!(args.package.is_none());
        assert_eq!(
            args.extra_cargo_args,
            vec!["--release".to_owned(), "--quiet".to_owned()]
        );
    }
}
