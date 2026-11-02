//! Горячая перезагрузка приложения Light: супервизор `light dev`.
//!
//! Запускает приложение (`cargo run --features devtools`) как дочерний процесс,
//! следит за исходниками (`src/`, `templates/`, `Cargo.toml`), а при изменении
//! — сохраняет состояние приложения (команда `SAVE_STATE` через TCP-сервер
//! DevTools на порту 9229), убивает процесс и перезапускает его. При следующем
//! запуске приложение восстанавливает состояние из `.light/state.json`
//! (см. модуль `devtools` в `light-fw-core`).
//!
//! Надёжный мониторинг файлов использует `notify::PollWatcher` — периодическое
//! опросование файловой системы, которое работает на любых ФС, включая UNC и
//! сетевые диски, где нативный watcher (`recommended_watcher`) события не ловит.
//! Отложенный (debounce ~150 мс) повторный запуск координирует отдельный поток,
//! чтобы кратковременные серии событий не запускали много перезагрузок.

#[cfg(unix)]
use std::os::unix::process::ExitStatusExt as _;
#[cfg(windows)]
use std::os::windows::process::ExitStatusExt as _;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use notify::{Config, PollWatcher, RecursiveMode, Watcher as _};

const DEBOUNCE: Duration = Duration::from_millis(150);
const DEVTOOLS_PORT: u16 = 9229;
/// Интервал опроса файловой системы [`PollWatcher`]. На UNC- и сетевых дисках
/// нативный watcher события не ловит, поэтому используем периодическое
/// опросование (500 мс — разумный баланс между задержкой реакции и нагрузкой).
const POLL_INTERVAL: Duration = Duration::from_millis(500);
/// Максимум времени, которое наблюдатель игнорирует события, пока приложение
/// собирается/стартует. Защита от вечной блокировки: если порт 9229 так и не
/// открылся (например, сборка зависла), по истечении этого дедлайна наблюдение
/// всё равно возобновляется.
const STARTUP_IGNORE: Duration = Duration::from_secs(30);
/// Периодичность проверки открытия порта DevTools (9229) фоном.
const PORT_PROBE: Duration = Duration::from_millis(250);

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

/// Определяет корень пакета, который запускает `light dev`.
///
/// Если передан аргумент `package` (например `example-devtools-demo`), корень
/// находится через `cargo metadata --no-deps` — берётся родительский каталог
/// `manifest_path` нужного пакета. Это важно: без этого `watch_paths` слежил бы
/// за корнем воркспейса (где лежат все `examples/*`), а не за конкретным
/// примером, и изменения в `examples/<name>/` не отслеживались бы.
///
/// Если пакет не передан или `cargo metadata` завершился с ошибкой, возвращается
/// `cwd` (поведение «как раньше» — наблюдение текущего каталога).
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

/// Проверяет, открыт ли TCP-порт DevTools (на нём приложение принимает команду
/// `SAVE_STATE`). `TcpStream::connect` к свободному порту возвращает ошибку
/// мгновенно (connection refused), так что это дешёвая проверка.
fn is_port_open(port: u16) -> bool {
    use std::net::TcpStream;
    TcpStream::connect(("127.0.0.1", port)).is_ok()
}

/// Живое состояние наблюдателя: окно игнорирования при сборке/старте
/// (`is_starting`) и накопленное «изменение было во время сборки»
/// (`pending_change`), которое нужно применить сразу после сброса окна.
#[derive(Debug, Default)]
pub struct WatchState {
    /// Игнорировать события (приложение ещё собирается или только стартует).
    pub is_starting: bool,
    /// Зафиксировано изменение во время `is_starting`; применить после сброса.
    pub pending_change: bool,
}

impl WatchState {
    /// Создаёт новое состояние: окно игнорирования включено.
    pub fn starting() -> Self {
        Self {
            is_starting: true,
            pending_change: false,
        }
    }
}

/// Логика обработки одного события наблюдателя с учётом состояния
/// [`WatchState`]. Отделена от потоков, чтобы юнит-тестить без запуска
/// процесса и файлового watcher'а.
///
/// Возвращает `true`, если после события следует отправить сигнал рестарта
/// (только если изменение реально произошло и окно игнорирования закрыто).
///
/// Правила:
/// * во время `is_starting` событие не приводит к рестарту, но запоминается —
///   флаг `pending_change` поднимается (защита от гонки: пользователь мог
///   править файл, пока компилятор ещё собирает `light-fw-core`, а порт 9229
///   не слушается);
/// * после сброса `is_starting` накопленный `pending_change` применяется сразу
///   и сбрасывается — один перезапуск за накопленную серию изменений;
/// * вне окна любое подходящее событие даёт рестарт.
pub fn handle_watch_event(state: &mut WatchState, is_relevant: bool) -> bool {
    if !is_relevant {
        return false;
    }
    if state.is_starting {
        // Изменение во время сборки: не рестартуем сейчас, но помним о нём.
        state.pending_change = true;
        false
    } else if state.pending_change {
        // Окно закрылось, но до этого был зафиксирован чейдж — применяем.
        state.pending_change = false;
        true
    } else {
        // Прямой чейдж вне окна.
        true
    }
}

/// Убивает дочерний процесс и дожидается его завершения.
///
/// После `wait()` даём ОС короткую паузу, чтобы освободить хендлы на `.rmeta`
/// и `.pdb` (Windows AV / антивирус часто держит файл ещё мгновение): без этой
/// паузы следующий `cargo run` может упасть с `os error 5` (Access Denied).
pub fn kill_child(child: &mut Child) {
    if let Err(e) = child.kill() {
        eprintln!("light dev: не удалось завершить дочерний процесс: {e}");
    }
    let _ = child.wait();
    // Пауза для освобождения файловых хендлов ОС/автовикором.
    thread::sleep(Duration::from_millis(500));
}

/// Точка входа команды `light dev`: запускает дочерний процесс и цикл
/// наблюдения. Возвращает код завершения приложения (не `anyhow`, чтобы ядро
/// CLI не тягло зависимость, которой нет в его манифесте).
///
/// Не вызывается автоматически из других частей библиотеки — только из
/// бинарного `light-fw-cli` (см. `cmd_dev` в `light-cli`).
pub fn run(args: &DevArgs) -> std::result::Result<i32, std::io::Error> {
    let cwd = std::env::current_dir()?;
    // Корень пакета: если передан `package`, ищем его через `cargo metadata`,
    // иначе — текущий каталог. Именно этот корень передаётся в `watch_paths`.
    let package_root = resolve_package_root(args.package.as_deref(), &cwd);
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
    // Флаг завершения: общий между главным потоком, наблюдателем и
    // фоном-проверкой порта. Атомарный (в отличие от mpsc::Receiver) — легко
    // шараится между несколькими тредами без clone() канала.
    let done_flag = Arc::new(AtomicBool::new(false));
    // Состояние наблюдателя (is_starting/pending_change) и канал на его
    // обновление. Главный поток решает, когда закрывать окно игнорирования:
    // как только порт 9229 открылся или прошёл STARTUP_IGNORE.
    let (state_tx, state_rx) = mpsc::channel::<WatchState>();
    let _ = state_tx.send(WatchState::starting());

    let watch_paths_copy = watch_paths(&package_root);
    eprintln!("light dev: watching {:?}", watch_paths_copy);
    let done_flag_watch = done_flag.clone();
    let watcher = thread::spawn(move || {
        if watch_paths_copy.is_empty() {
            eprintln!("light dev: нет путей для наблюдения (нет src/, templates/, Cargo.toml)");
            return;
        }
        let (tx, rx) = mpsc::channel::<notify::Result<notify::Event>>();
        // PollWatcher: периодическое опросование файловой системы. В отличие от
        // нативного `recommended_watcher`, работает на UNC- и сетевых дисках,
        // где нативный механизм события просто не ловит.
        let config = Config::default().with_poll_interval(POLL_INTERVAL);
        let mut watcher = match PollWatcher::new(
            move |res| {
                let _ = tx.send(res);
            },
            config,
        ) {
            Ok(w) => w,
            Err(e) => {
                eprintln!("light dev: не удалось создать файловый наблюдатель: {e}");
                return;
            }
        };
        for path in &watch_paths_copy {
            if let Err(e) = watcher.watch(path, RecursiveMode::NonRecursive) {
                eprintln!("light dev: не удалось следить за {}: {e}", path.display());
            }
        }
        // Начальное состояние: is_starting = true. Главное окно наблюдателя
        // закрывается, когда главный поток шлёт обновление (порт 9229 открыт
        // либо дедлайн STARTUP_IGNORE прошёл).
        let mut state = match state_rx.recv() {
            Ok(s) => s,
            Err(_) => return,
        };
        // Сбрасывает дебаунс; актуально только вне окна `is_starting`.
        let mut last_event: Option<Instant> = None;
        loop {
            // 1) Завершение от основного потока.
            if done_flag_watch.load(Ordering::Relaxed) {
                break;
            }
            // 2) Обновление состояния от главного потока. Прерываем цикл
            //    немедленно и применяем накопленный pending_change сразу.
            if let Ok(new_state) = state_rx.try_recv() {
                state = new_state;
                if !state.is_starting && state.pending_change {
                    eprintln!("light dev: применяю изменение, зафиксированное во время сборки");
                    let _ = restart_tx.send(Instant::now());
                    state.pending_change = false;
                }
                continue;
            }
            // 3) События наблюдателя.
            match rx.recv_timeout(Duration::from_millis(50)) {
                Ok(Ok(event)) => {
                    // Игнорируем события вне watched-путей.
                    let relevant = event
                        .paths
                        .iter()
                        .any(|p| watch_paths_copy.iter().any(|w| p.starts_with(w)));
                    if !relevant {
                        continue;
                    }
                    eprintln!(
                        "light dev: change detected at {:?}{}",
                        event.paths,
                        if state.is_starting {
                            " (во время сборки — запомню)"
                        } else {
                            ""
                        }
                    );
                    // Чистая машина состояний: во время is_starting сохраняет
                    // изменение в pending_change и не рестартует; вне окна —
                    // рестарт (либо применение pending_change после его сброса).
                    if handle_watch_event(&mut state, true) {
                        let _ = restart_tx.send(Instant::now());
                        // Дебаунс на будущее (на случай серии чейджей).
                        last_event = Some(Instant::now());
                    } else if state.is_starting {
                        // Изменение во время сборки: пока просто помним.
                        // pending_change уже поставлен в handle_watch_event.
                    } else if state.pending_change {
                        // Окно только что закрылось, накопленное изменение
                        // применено; обнуляем дебаунс, чтобы последующие
                        // чейджи снова считались.
                        state.pending_change = false;
                        last_event = Some(Instant::now());
                    }
                }
                Ok(Err(e)) => {
                    eprintln!("light dev: ошибка наблюдателя: {e}");
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    // Дебаунс: серия чейджей вне окна — один рестарт.
                    if let Some(t) = last_event
                        && t.elapsed() >= DEBOUNCE
                        && !state.is_starting
                        && !state.pending_change
                    {
                        last_event = None;
                        let _ = restart_tx.send(Instant::now());
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
    });

    // Фоновая проверка открытия порта DevTools (9229). Как только приложение
    // стало принимать команды — снимаем окно игнорирования наблюдателя.
    // Завершается после первого успеха (порт уже слушается).
    {
        let state_probe_tx = state_tx.clone();
        let done_probe_flag = done_flag.clone();
        thread::spawn(move || {
            let start = Instant::now();
            loop {
                if done_probe_flag.load(Ordering::Relaxed) {
                    break;
                }
                if is_port_open(DEVTOOLS_PORT) {
                    eprintln!(
                        "light dev: порт DevTools {DEVTOOLS_PORT} открыт — слежу за изменениями"
                    );
                    let _ = state_probe_tx.send(WatchState {
                        is_starting: false,
                        pending_change: true,
                    });
                    break;
                }
                if start.elapsed() >= STARTUP_IGNORE {
                    // Дедлайн обработает главный поток; этот тред молча
                    // завершается.
                    break;
                }
                thread::sleep(PORT_PROBE);
            }
        });
    }

    loop {
        // Дедлайн защиты от вечной блокировки: если порт 9229 так и не
        // открылся за STARTUP_IGNORE (сборка зависла), закрываем окно
        // игнорирования наблюдателя, чтобы он снова начал ловить чейджи.
        let mut clear_deadline: Option<Instant> = Some(Instant::now() + STARTUP_IGNORE);
        // Внутренний цикл опроса: выходим в наружный цикл по первому событию
        // (перезапуск от наблюдателя либо завершение дочернего процесса).
        enum Ev {
            Restart,
            Exited(std::process::ExitStatus),
        }
        let ev = loop {
            // Тик дедлайна: один раз за каждый прогон внутреннего цикла.
            if let Some(deadline) = clear_deadline
                && Instant::now() >= deadline
            {
                eprintln!(
                    "light dev: порт DevTools {} не открылся за {:?} — снимаю окно \
                     игнорирования наблюдателя",
                    DEVTOOLS_PORT, STARTUP_IGNORE
                );
                let _ = state_tx.send(WatchState {
                    is_starting: false,
                    pending_change: true,
                });
                clear_deadline = None;
            }
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
                done_flag.store(true, Ordering::Relaxed);
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
                // Пересоздаём дочерний процесс и сбрасываем наблюдателя в
                // is_starting: новое приложение снова сначала соберётся/запустится,
                // поэтому окно игнорирования открывается заново.
                match spawn_cmd(args, &cwd).spawn() {
                    Ok(c) => {
                        child = c;
                        // Сбрасываем наблюдателя в is_starting. Дедлайн
                        // `clear_deadline` — новый каждый прогон внешнего цикла
                        // (в начале тела `let mut ... = Some(now + STARTUP_IGNORE)`),
                        // поэтому явного сброса здесь не нужно.
                        let _ = state_tx.send(WatchState::starting());
                        eprintln!("light dev: приложение перезапущено");
                    }
                    Err(e) => {
                        eprintln!("light dev: не удалось перезапустить приложение: {e}");
                        done_flag.store(true, Ordering::Relaxed);
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
    use super::{WatchState, handle_watch_event, parse_dev_command, resolve_package_root};

    /// Событие вне watched-путей: рестарта нет, состояние не меняется.
    #[test]
    fn watch_event_ignored_outside_watched_paths() {
        let mut s = WatchState::starting();
        assert!(!handle_watch_event(&mut s, false));
        assert!(s.is_starting);
        assert!(!s.pending_change);
    }

    /// Изменение во время is_starting: рестарт не отправляется, но
    /// pending_change запоминается.
    #[test]
    fn watch_event_during_startup_sets_pending() {
        let mut s = WatchState::starting();
        assert!(!handle_watch_event(&mut s, true));
        assert!(s.is_starting);
        assert!(s.pending_change);
    }

    /// Серия изменений во время is_starting: pending_change остаётся true
    /// (не «всплывает»), рестартов не происходит.
    #[test]
    fn watch_event_multiple_during_startup_still_no_restart() {
        let mut s = WatchState::starting();
        assert!(!handle_watch_event(&mut s, true));
        assert!(!handle_watch_event(&mut s, true));
        assert!(!handle_watch_event(&mut s, true));
        assert!(s.is_starting);
        assert!(s.pending_change);
    }

    /// После закрытия is_starting (и применения pending_change) прямой
    /// чейдж даёт рестарт.
    #[test]
    fn watch_event_direct_after_startup_clear() {
        let mut s = WatchState {
            is_starting: false,
            pending_change: false,
        };
        assert!(handle_watch_event(&mut s, true));
        assert!(!s.is_starting);
        assert!(!s.pending_change);
    }

    /// Ключевая сценарий ТЗ: изменение во время сборки → закрытие окна →
    /// накопленное изменение применяется одним рестартом.
    #[test]
    fn watch_event_pending_applied_after_window_close() {
        let mut s = WatchState::starting();
        // Чейдж во время сборки.
        assert!(!handle_watch_event(&mut s, true));
        assert!(s.pending_change);
        // Главный поток закрывает окно (порт открыт / дедлайн прошёл).
        s.is_starting = false;
        // Наблюдатель видит, что pending_change есть, и применяет его.
        assert!(handle_watch_event(&mut s, true));
        assert!(!s.pending_change);
        assert!(!s.is_starting);
    }

    /// Без имени пакета корнем становится текущий каталог (быстрый путь без
    /// запуска `cargo metadata`).
    #[test]
    fn package_root_without_package_is_cwd() {
        let root = resolve_package_root(None, std::path::Path::new("/tmp/workspace"));
        assert_eq!(root, std::path::PathBuf::from("/tmp/workspace"));
    }

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
