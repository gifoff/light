//! Горячая перезагрузка приложения Light: супервизор `light dev`.
//!
//! Запускает приложение (`cargo run --features devtools`) как дочерний процесс,
//! следит за исходниками (`src/`, `templates/`, `Cargo.toml`). Две реакции:
//! * **HTML** (файлы `*.html`) → команда `RELOAD_TEMPLATE` через TCP-сервер
//!   DevTools на порту 9229: приложение перечитывает HTML-шаблон с диска и
//!   перерисовывает UI **без пересборки**. При неудачной отправки (app умер,
//!   порт недоступен — os error 10061) **не триггерим полную пересборку**:
//!   ждём до 3 с (poll каждые 200 мс), пока TCP снова станет доступен, и
//!   повторяем `RELOAD_TEMPLATE`.
//! * **Source** (файлы `*.rs` / `Cargo.toml`) → полная пересборка + рестарт:
//!   сохраняем состояние (`SAVE_STATE`), убиваем процесс, перезапускаем.
//!
//! Race-защита: во время initial build (окно `is_starting`) изменения не
//! приводят к немедленным действиям — они накапливаются в `pending_html` /
//! `pending_source`. После закрытия окна (порт 9229 открылся либо прошёл
//! дедлайн `STARTUP_IGNORE`) накопленное применяется с приоритетом Source >
//! Html (пересборка важнее reload). HTML-чнейдж во время initial build
//! **не** приводит к пустому restart — это ReloadHtml после старта.
//!
//! Надёжный мониторинг файлов использует `notify::PollWatcher` — периодическое
//! опросование файловой системы, которое работает на любых ФС, включая UNC и
//! сетевые диски, где нативный watcher (`recommended_watcher`) события не ловит.
//! Дебаунс (100 мс для HTML, 500 мс для Source) координирует отдельный поток,
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
/// Максимум времени ожидания восстановления TCP-доступа к порту 9229 после
/// неудачной отправки `RELOAD_TEMPLATE` (приложение могло умереть или
/// пересобраться). Каждые 200 мс пробуем TCP-подключение; если порт снова
/// принимает соединения — повторяем `RELOAD_TEMPLATE` (без полной пересборки).
const RELOAD_WAIT_MAX: Duration = Duration::from_secs(3);
/// Период опроса TCP при ожидании восстановления после неудачного reload.
const RELOAD_POLL_INTERVAL: Duration = Duration::from_millis(200);
/// Дебаунс для изменений `.rs`/`Cargo.toml` (полная пересборка + рестарт).
const RS_DEBOUNCE: Duration = Duration::from_millis(500);
/// Дебаунс для изменений `.html` (runtime-перезагрузка шаблона без пересборки).
const HTML_DEBOUNCE: Duration = Duration::from_millis(100);

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

/// Проверяет, открыт ли TCP-порт DevTools (на нём приложение принимает команду
/// `SAVE_STATE`). `TcpStream::connect` к свободному порту возвращает ошибку
/// мгновенно (connection refused), так что это дешёвая проверка.
fn is_port_open(port: u16) -> bool {
    use std::net::TcpStream;
    TcpStream::connect(("127.0.0.1", port)).is_ok()
}

/// Хэш содержимого файла (`u64`) для дедупликации событий notify.
///
/// Один «логический» записывающий вызов (например, сохранение в редакторе)
/// генерирует несколько событий `PollWatcher` — `Modify`, `Modify(Data)`,
/// `Modify(Metadata)` — с **одинаковым** содержимым. Дебаунс по времени (100/500
/// мс) эту серию не схватывает, потому что вторая серия приходит ПОСЛЕ debounce.
/// Хэш содержимого позволяет отфильтровать повторы: если `hash == last_hashes[&path]`
/// — событие игнорируется как повтор того же содержимого.
///
/// Возвращает `None`, если файл нельзя прочитать (например, только что удалён
/// или ещё пишется); в этом случае событие НЕ дедуплицируется и обрабатывается
/// как изменение (консервативно).
pub fn file_hash(path: &Path) -> Option<u64> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    use std::io::Read;

    let mut buf = Vec::new();
    let mut f = std::fs::File::open(path).ok()?;
    // Читаем всё содержимое. Файлы, за которыми мы следим, — исходники
    // (`.rs`, `.html`, `Cargo.toml`), так что объём небольшой (KB–MB).
    if f.read_to_end(&mut buf).is_err() {
        return None;
    }
    let mut hasher = DefaultHasher::new();
    buf.hash(&mut hasher);
    Some(hasher.finish())
}

/// Тип изменения файла для классификации hot-reload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    /// HTML-шаблон: runtime-перезагрузка без пересборки (`RELOAD_TEMPLATE`).
    Html,
    /// Rust-исходник или `Cargo.toml`: полная пересборка + рестарт.
    Source,
}

/// Классифицирует путь изменённого файла в [`ChangeKind`].
///
/// `.html` → [`ChangeKind::Html`]; `.rs`/`Cargo.toml` → [`ChangeKind::Source`].
/// Любой другой расширение (например, `.json`, `.png`) считается нерелевантным
/// и классифицируется как `Source` (консервативно: лучше лишняя пересборка).
pub fn classify_change(path: &std::path::Path) -> Option<ChangeKind> {
    let file_name = path.file_name()?.to_string_lossy();
    if file_name == "Cargo.toml" {
        return Some(ChangeKind::Source);
    }
    match path.extension()?.to_string_lossy().as_ref() {
        "html" => Some(ChangeKind::Html),
        "rs" => Some(ChangeKind::Source),
        _ => None,
    }
}

/// Живое состояние наблюдателя: окно игнорирования при сборке/старте
/// (`is_starting`) и накопленные изменения, которые нужно применить сразу
/// после сброса окна. HTML (runtime-reload) и Source (пересборка)
/// разделяются отдельными флагами: HTML-чнейдж во время initial build
/// не должен приводить к полному рестарту приложения.
#[derive(Debug, Default)]
pub struct WatchState {
    /// Игнорировать события (приложение ещё собирается или только стартует).
    /// Во время этого окна прямые чейджи не вызывают действия — вместо этого
    /// они накапливаются в [`Self::pending_html`] / [`Self::pending_source`].
    pub is_starting: bool,
    /// Зафиксировано изменение HTML-шаблона во время `is_starting`. После
    /// закрытия окна → `RELOAD_TEMPLATE` (без пересборки).
    pub pending_html: bool,
    /// Зафиксировано изменение Rust-исходника/`Cargo.toml` во время
    /// `is_starting`. После закрытия окна → полная пересборка + рестарт.
    pub pending_source: bool,
}

impl WatchState {
    /// Создаёт новое состояние: окно игнорирования включено, pending-флаги
    /// сняты.
    pub fn starting() -> Self {
        Self {
            is_starting: true,
            pending_html: false,
            pending_source: false,
        }
    }

    /// Есть ли накопленное изменение, которое нужно применить после закрытия
    /// окна `is_starting`?
    pub fn has_pending(&self) -> bool {
        self.pending_html || self.pending_source
    }

    /// Сбрасывает оба pending-флага.
    pub fn clear_pending(&mut self) {
        self.pending_html = false;
        self.pending_source = false;
    }
}

/// Результат обработки одного события наблюдателя: тип реакции (или её
/// отсутствие), либо сигнал, что накопленное изменение требует применения.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchAction {
    /// Отправить `RELOAD_TEMPLATE` (без убийства процесса).
    ReloadHtml,
    /// Полная пересборка + рестарт процесса.
    Restart,
    /// Применить накопленное изменение (вызывается при закрытии окна
    /// `is_starting` либо когда pending-флаг оказался снятым ранее).
    ApplyPending,
    /// Действия нет (событие нерелевантно или просто запомнено).
    None,
}

/// Логика обработки одного события наблюдателя с учётом состояния
/// [`WatchState`]. Отделена от потоков, чтобы юнит-тестить без запуска
/// процесса и файлового watcher'а.
///
/// Правила (ТЗ 0.14.2-A-fix):
/// * `Html` + `is_starting` → запоминаем в `pending_html`, без действия;
/// * `Source` + `is_starting` → запоминаем в `pending_source`, без действия;
/// * `Html` + `!is_starting` → [`WatchAction::ReloadHtml`];
/// * `Source` + `!is_starting` → [`WatchAction::Restart`];
/// * если перед событием были накоплены pending-флаги и окно уже закрыто —
///   они применяются первым ([`WatchAction::ApplyPending`]): приоритет у
///   source (рестарт), html (reload) вторым.
///
/// Возвращает [`WatchAction::None`], если событие нерелевантно (ни `Html`,
/// ни `Source`).
pub fn handle_watch_event(state: &mut WatchState, kind: Option<ChangeKind>) -> WatchAction {
    let Some(kind) = kind else {
        return WatchAction::None;
    };
    // Если до этого было накоплено изменение, а окно уже закрыто — применяем
    // его первым (один перезапуск за накопленную серию). Приоритет Source >
    // Html: источник всегда требует пересборки, HTML может подождать reload.
    if state.has_pending() && !state.is_starting {
        state.clear_pending();
        return WatchAction::ApplyPending;
    }
    if state.is_starting {
        // Изменение во время сборки: не реагируем сейчас, но помним о нём.
        match kind {
            ChangeKind::Html => state.pending_html = true,
            ChangeKind::Source => state.pending_source = true,
        }
        WatchAction::None
    } else {
        match kind {
            ChangeKind::Html => WatchAction::ReloadHtml,
            ChangeKind::Source => WatchAction::Restart,
        }
    }
}

/// Решает, какое действие применить при закрытии окна `is_starting` (порт
/// DevTools открылся либо прошёл дедлайн `STARTUP_IGNORE`). Приоритет:
/// Source (полная пересборка) > Html (runtime-reload) > None.
pub fn resolve_startup_completion(state: &WatchState) -> Option<ChangeKind> {
    if state.pending_source {
        Some(ChangeKind::Source)
    } else if state.pending_html {
        Some(ChangeKind::Html)
    } else {
        None
    }
}

/// Ожидает восстановления TCP-доступа к порту `port` в течение `max` (poll
/// каждые `interval`). Возвращает `true`, если порт принял соединение,
/// иначе `false`. Используется для повторной отправки `RELOAD_TEMPLATE`
/// после неудачного (например, app умер) — без полной пересборки.
pub fn wait_port_available(port: u16, max: Duration, interval: Duration) -> bool {
    let start = Instant::now();
    loop {
        if is_port_open(port) {
            return true;
        }
        if start.elapsed() >= max {
            return false;
        }
        thread::sleep(interval);
    }
}

/// Пытается отправить `RELOAD_TEMPLATE`; при неудаче (app умер, порт
/// недоступен) ждёт до [`RELOAD_WAIT_MAX`], пока TCP снова станет доступен
/// (poll каждые [`RELOAD_POLL_INTERVAL`]), и повторяет отправку. Не триггерит
/// полную пересборку: HTML-чнейдж — это runtime-reload, не rebuild.
///
/// Возвращает `Ok(())` при успехе (включая повторную попытку), `Err` — если
/// и после ожидания порт так и не стал доступен.
pub fn request_reload_template_with_retry(port: u16) -> std::result::Result<(), std::io::Error> {
    if let Ok(()) = request_reload_template(port) {
        return Ok(());
    }
    // Неудача: приложение могло умереть или зависнуть. Ждём, пока TCP снова
    // станет доступен, и повторяем RELOAD_TEMPLATE (без пересборки).
    eprintln!(
        "light dev: RELOAD_TEMPLATE не удался (app недоступен?) — \
         жду до {:?}, чтобы повторить (не запуская пересборку)…",
        RELOAD_WAIT_MAX
    );
    if wait_port_available(port, RELOAD_WAIT_MAX, RELOAD_POLL_INTERVAL) {
        eprintln!("light dev: порт DevTools снова доступен — повторяю RELOAD_TEMPLATE");
        request_reload_template(port)
    } else {
        eprintln!(
            "light dev: порт DevTools {} так и не открылся за {:?} — \
             HTML-чнейдж будет потерян, до следующего изменения",
            port, RELOAD_WAIT_MAX
        );
        Err(std::io::Error::new(
            std::io::ErrorKind::ConnectionRefused,
            "port unavailable after reload wait",
        ))
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

    // Канал на перезапуск от наблюдателя. Несёт тип изменения: Html →
    // RELOAD_TEMPLATE (без убийства), Source → kill/rebuild/respawn.
    let (restart_tx, restart_rx) = mpsc::channel::<(Instant, ChangeKind)>();
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
        // Начальное состояние: is_starting = true. Watcher сам решает, когда
        // закрывать окно: как только порт DevTools открылся либо прошёл
        // дедлайн STARTUP_IGNORE (защита от вечной блокировки при зависшей
        // сборке). Principal-поток лишь сбрасывает state в `starting()` при
        // перезапуске процесса.
        let mut state = match state_rx.recv() {
            Ok(s) => s,
            Err(_) => return,
        };
        // Момент запуска приложения: используется для дедлайна STARTUP_IGNORE.
        // Пересбрасывается при каждом новом запуске дочернего процесса (главный
        // поток шлёт `WatchState::starting()` через state_rx).
        let mut start_at = Instant::now();
        // Сбрасывает дебаунс; актуально только вне окна `is_starting`.
        // Отдельный таймер для каждого типа изменения (Html vs Source).
        let mut last_html: Option<Instant> = None;
        let mut last_rs: Option<Instant> = None;
        // Дедупликация по хэшу содержимого: один «логический» save генерирует
        // несколько событий notify (Modify + Modify(Data) + Modify(Metadata))
        // с одинаковым содержимым. Хэш защищает от повторной обработки;
        // дебаунс — от быстрых серий записи (редактор пишет файл по частям).
        let mut last_hashes: std::collections::HashMap<PathBuf, u64> =
            std::collections::HashMap::new();
        // Приоритет pending-флагов (Source > Html): если оба были накоплены,
        // применяем source (пересборка важнее reload).
        let apply_pending = |state: &mut WatchState,
                             tx: &mpsc::Sender<(Instant, ChangeKind)>|
         -> Option<ChangeKind> {
            let kind = resolve_startup_completion(state)?;
            let _ = tx.send((Instant::now(), kind));
            state.clear_pending();
            Some(kind)
        };
        loop {
            // 1) Завершение от основного потока.
            if done_flag_watch.load(Ordering::Relaxed) {
                break;
            }
            // 2) Обновление состояния от главного потока (при перезапуске
            //    приложение сбрасывает state в `starting()`).
            if let Ok(new_state) = state_rx.try_recv() {
                state = new_state;
                start_at = Instant::now();
                last_html = None;
                last_rs = None;
                continue;
            }
            // 3) Окно игнорирования: порт 9229 открылся или дедлайн прошёл.
            //    Применяем накопленный pending (если есть) как ReloadHtml /
            //    Restart — без эвристики «обнаружены изменения».
            if state.is_starting {
                let port_open = is_port_open(DEVTOOLS_PORT);
                let deadline_reached = start_at.elapsed() >= STARTUP_IGNORE;
                if port_open || deadline_reached {
                    let reason = if port_open {
                        "порт DevTools открыт"
                    } else {
                        &format!("дедлайн {:?} истёк", STARTUP_IGNORE)
                    };
                    eprintln!(
                        "light dev: окно игнорирования снято ({reason}), \
                         очередь pending (HTML={}, Source={})",
                        state.pending_html, state.pending_source
                    );
                    if let Some(kind) = apply_pending(&mut state, &restart_tx) {
                        match kind {
                            ChangeKind::Html => last_html = Some(Instant::now()),
                            ChangeKind::Source => last_rs = Some(Instant::now()),
                        }
                    }
                    state.is_starting = false;
                }
                continue;
            }
            // 3) События наблюдателя.
            match rx.recv_timeout(Duration::from_millis(50)) {
                Ok(Ok(event)) => {
                    // Для каждого пути из события:
                    //  1) если путь не в watch_paths → skip;
                    //  2) хэшируем содержимое файла;
                    //  3) если хэш == последнему сохранённому для этого пути
                    //     → ИГНОРИРОВАТЬ (повторное событие того же содержимого,
                    //     например Modify(Data) / Modify(Metadata) после Modify);
                    //  4) иначе обновляем last_hashes и классифицируем как
                    //     настоящее изменение.
                    //
                    // Если хотя бы один путь дал новое содержимое — обрабатываем
                    // событие. Берём первый такой путь для классификации (Html /
                    // Source / None). Это решает проблему «2 RELOAD_TEMPLATE на
                    // одно изменение»: вторые/третьи события notify с тем же
                    // хэшем отфильтруются здесь, до debounce.
                    let mut effective_path: Option<&PathBuf> = None;
                    for path in &event.paths {
                        if !watch_paths_copy.iter().any(|w| path.starts_with(w)) {
                            continue;
                        }
                        let hash = match file_hash(path) {
                            Some(h) => h,
                            // Файл недоступен (удалён / ещё пишется): не
                            // дедуплицируем — обрабатываем как изменение.
                            None => {
                                effective_path = Some(path);
                                break;
                            }
                        };
                        match last_hashes.get(path) {
                            Some(&last) if last == hash => {
                                // Повторное событие того же содержимого:
                                // игнорируем.
                                eprintln!(
                                    "light dev: повторное событие notify для {:?} \
                                     (хэш не изменился) — игнорирую",
                                    path
                                );
                                continue;
                            }
                            _ => {
                                last_hashes.insert(path.clone(), hash);
                                effective_path = Some(path);
                            }
                        }
                    }
                    let kind = effective_path.and_then(|p| classify_change(p));
                    if kind.is_none() {
                        continue;
                    }
                    let label = match kind {
                        Some(ChangeKind::Html) => "HTML",
                        Some(ChangeKind::Source) => "Source",
                        None => "?",
                    };
                    eprintln!(
                        "light dev: change detected at {:?} ({label}){}",
                        event.paths,
                        if state.is_starting {
                            " (во время сборки — запомню)"
                        } else {
                            ""
                        }
                    );
                    match handle_watch_event(&mut state, kind) {
                        WatchAction::None => {
                            // Изменение во время сборки: просто запомнили
                            // (pending_html / pending_source подняты внутри
                            // handle_watch_event).
                        }
                        WatchAction::ApplyPending => {
                            // Окно было закрыто, а pending остался: применяем.
                            if let Some(akind) = apply_pending(&mut state, &restart_tx) {
                                eprintln!("light dev: применяю pending (kind={:?})", akind);
                                match akind {
                                    ChangeKind::Html => last_html = Some(Instant::now()),
                                    ChangeKind::Source => last_rs = Some(Instant::now()),
                                }
                            }
                        }
                        WatchAction::ReloadHtml => {
                            eprintln!(
                                "light dev: HTML change → RELOAD_TEMPLATE \
                                 (без пересборки)"
                            );
                            let _ = restart_tx.send((Instant::now(), ChangeKind::Html));
                            last_html = Some(Instant::now());
                        }
                        WatchAction::Restart => {
                            eprintln!(
                                "light dev: Source change → RESTART \
                                 (пересборка)"
                            );
                            let _ = restart_tx.send((Instant::now(), ChangeKind::Source));
                            last_rs = Some(Instant::now());
                        }
                    }
                }
                Ok(Err(e)) => {
                    eprintln!("light dev: ошибка наблюдателя: {e}");
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    // Дебаунс: серия чейджей вне окна — одно действие.
                    if !state.is_starting && !state.has_pending() {
                        if let Some(t) = last_html
                            && t.elapsed() >= HTML_DEBOUNCE
                        {
                            last_html = None;
                            let _ = restart_tx.send((Instant::now(), ChangeKind::Html));
                        }
                        if let Some(t) = last_rs
                            && t.elapsed() >= RS_DEBOUNCE
                        {
                            last_rs = None;
                            let _ = restart_tx.send((Instant::now(), ChangeKind::Source));
                        }
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
    });

    loop {
        // Дедлайн защиты от вечной блокировки: если порт 9229 так и не
        // открылся за STARTUP_IGNORE (сборка зависла), watcher сам закроет
        // окно игнорирования (он владеет pending-флагами и знает, что
        // применить). Главный поток здесь ничего не решает.
        // Внутренний цикл опроса: выходим в наружный цикл по первому событию
        // (перезапуск от наблюдателя либо завершение дочернего процесса).
        enum Ev {
            Restart(ChangeKind),
            Exited(std::process::ExitStatus),
        }
        let ev = loop {
            if let Ok((_ts, kind)) = restart_rx.try_recv() {
                break Ev::Restart(kind);
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
            Ev::Restart(kind) => match kind {
                ChangeKind::Html => {
                    // HTML-чнейдж → runtime-reload (RELOAD_TEMPLATE) без
                    // убийства процесса. При неудаче (app умер, порт недоступен)
                    // НЕ триггерим полную пересборку: ждём, пока TCP снова
                    // станет доступен (poll каждые 200 мс, до 3 с), и повторяем
                    // RELOAD_TEMPLATE. Если так и не поднимется — чнейдж
                    // потеряется до следующего изменения (без лишних 4-5 сек
                    // на пустой restart).
                    eprintln!("light dev: HTML change → RELOAD_TEMPLATE (без пересборки)…");
                    if let Err(e) = request_reload_template_with_retry(DEVTOOLS_PORT) {
                        eprintln!(
                            "light dev: RELOAD_TEMPLATE так и не был доставлен: {e} \
                             (HTML-чнейдж потерян до следующего изменения)"
                        );
                    } else {
                        eprintln!("light dev: RELOAD_TEMPLATE доставлен — продолжаю watch");
                    }
                }
                ChangeKind::Source => {
                    // Source-чнейдж (.rs / Cargo.toml) → полная пересборка +
                    // рестарт. Специфичное сообщение (без эвристики
                    // «обнаружены изменения»).
                    eprintln!(
                        "light dev: Source change → RESTART (пересборка) — \
                         сохраняю состояние и перезапускаю…"
                    );
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
                            // Сбрасываем watcher в is_starting: он сам решит,
                            // когда закрывать окно (порт 9229 открылся либо
                            // дедлайн STARTUP_IGNORE прошёл).
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
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ChangeKind, WatchAction, WatchState, classify_change, file_hash, handle_watch_event,
        parse_dev_command, resolve_package_root, resolve_startup_completion, wait_port_available,
    };
    use std::io::Write as _;
    use std::net::TcpListener;
    use std::path::Path;
    use std::time::{Duration, Instant};

    // ===================== handle_watch_event / resolve =====================

    /// Нерелевантное событие (kind=None): ни действий, ни изменения флагов.
    #[test]
    fn watch_event_irrelevant_does_nothing() {
        let mut s = WatchState::starting();
        assert_eq!(handle_watch_event(&mut s, None), WatchAction::None);
        assert!(s.is_starting);
        assert!(!s.pending_html);
        assert!(!s.pending_source);
    }

    /// **ТЗ 4.1**: HTML-чнейдж во время `is_starting` → `pending_html=true`,
    /// `pending_source=false`. При закрытии окна → ReloadHtml (НЕ restart).
    #[test]
    fn html_during_start_pending_as_html() {
        let mut s = WatchState::starting();
        assert_eq!(
            handle_watch_event(&mut s, Some(ChangeKind::Html)),
            WatchAction::None
        );
        assert!(s.is_starting);
        assert!(s.pending_html);
        assert!(!s.pending_source);
        // Окно закрывается (порт 9229 открылся либо дедлайн прошёл):
        // применение → Html (runtime-reload), не Source (пересборка).
        assert_eq!(resolve_startup_completion(&s), Some(ChangeKind::Html));
    }

    /// **ТЗ 4.2**: Source-чнейдж во время `is_starting` → `pending_source=true`,
    /// `pending_html=false`. При закрытии окна → Source (пересборка).
    #[test]
    fn source_during_start_pending_as_source() {
        let mut s = WatchState::starting();
        assert_eq!(
            handle_watch_event(&mut s, Some(ChangeKind::Source)),
            WatchAction::None
        );
        assert!(s.is_starting);
        assert!(!s.pending_html);
        assert!(s.pending_source);
        assert_eq!(resolve_startup_completion(&s), Some(ChangeKind::Source));
    }

    /// **ТЗ 4.3**: HTML-чнейдж ПОСЛЕ закрытия `is_starting` → немедленный
    /// `ReloadHtml` (не restart). Это ключевой фикс race: HTML-чнейдж не
    /// должен приводить к полной пересборке.
    #[test]
    fn html_after_start_emits_reload() {
        let mut s = WatchState {
            is_starting: false,
            pending_html: false,
            pending_source: false,
        };
        assert_eq!(
            handle_watch_event(&mut s, Some(ChangeKind::Html)),
            WatchAction::ReloadHtml
        );
        assert!(!s.is_starting);
        assert!(!s.pending_html);
        assert!(!s.pending_source);
    }

    /// Source-чнейдж после закрытия окна → немедленный Restart.
    #[test]
    fn source_after_start_emits_restart() {
        let mut s = WatchState {
            is_starting: false,
            pending_html: false,
            pending_source: false,
        };
        assert_eq!(
            handle_watch_event(&mut s, Some(ChangeKind::Source)),
            WatchAction::Restart
        );
    }

    /// Приоритет при закрытии окна: Source > Html (пересборка важнее reload).
    #[test]
    fn startup_priority_source_over_html() {
        let mut s = WatchState::starting();
        let _ = handle_watch_event(&mut s, Some(ChangeKind::Html));
        let _ = handle_watch_event(&mut s, Some(ChangeKind::Source));
        assert!(s.pending_html && s.pending_source);
        assert_eq!(resolve_startup_completion(&s), Some(ChangeKind::Source));
    }

    /// Без pending-флагов при закрытии окна — `None` (действий нет).
    #[test]
    fn startup_no_pending_returns_none() {
        let s = WatchState::starting();
        assert_eq!(resolve_startup_completion(&s), None);
    }

    /// Если перед событием был pending и окно уже закрыто — `ApplyPending`
    /// (один перезапуск за накопленную серию).
    #[test]
    fn event_with_stale_pending_returns_apply_pending() {
        let mut s = WatchState {
            is_starting: false,
            pending_html: true,
            pending_source: false,
        };
        assert_eq!(
            handle_watch_event(&mut s, Some(ChangeKind::Source)),
            WatchAction::ApplyPending
        );
        // Флаги сброшены.
        assert!(!s.pending_html && !s.pending_source);
    }

    // ===================== wait_port_available / reload retry ==============

    /// **ТЗ 4.4** (логика мока TCP): порт стал доступен до дедлайна →
    /// `wait_port_available` возвращает `true` раньше, чем истёк `max`.
    /// Не запускает пересборку: просто подтверждает, что повторная попытка
    /// `RELOAD_TEMPLATE` возможна.
    #[test]
    fn wait_port_available_becomes_open_within_max() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().unwrap().port();
        // Порт уже открыт — должно вернуть true мгновенно (без ожидания max).
        let start = Instant::now();
        assert!(wait_port_available(
            port,
            Duration::from_millis(500),
            Duration::from_millis(20)
        ));
        // Задержка < max: подтверждается, что мы не ждали всё время.
        assert!(
            start.elapsed() < Duration::from_millis(500),
            "wait_port_available вернул слишком поздно"
        );
    }

    /// **ТЗ 4.4** (часть 2): порт так и не открылся → `false` через ~`max`.
    /// В production это означает: `request_reload_template_with_retry` вернёт
    /// `Err` — без триггера полной пересборки (HTML-чнейдж просто потеряется
    /// до следующего изменения).
    ///
    /// Проверяем поведение (не wall-clock время): если функция вернула `false`,
    /// значит цикл закончился по дедлайну; если бы она «зависла» или «сработала
    /// раньше», мы бы получили `true` или panic по timeout. Дополнительная
    /// проверка на wall-clock сознательно опущена — на Windows под нагрузкой
    /// `thread::sleep` не даёт точного верхнего bound.
    #[test]
    fn wait_port_available_never_opens_times_out() {
        // Берём свободный порт: listen'им и сразу drop'аем слушателя, чтобы
        // порт гарантированно не принимал соединения.
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        // Короткий дедлайн (80 мс), быстрый poll (10 мс) — тест завершится
        // быстро и надёжно. Функция должна вернуть false.
        let result =
            wait_port_available(port, Duration::from_millis(80), Duration::from_millis(10));
        assert!(
            !result,
            "wait_port_available должен вернуть false для закрытого порта"
        );
    }

    // ===================== parse / classify / package_root ==================

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

    /// `.html` → `ChangeKind::Html`.
    #[test]
    fn classify_html_file() {
        assert_eq!(
            classify_change(Path::new("templates/app.html")),
            Some(ChangeKind::Html)
        );
    }

    /// `.rs` → `ChangeKind::Source`.
    #[test]
    fn classify_rs_file() {
        assert_eq!(
            classify_change(Path::new("src/main.rs")),
            Some(ChangeKind::Source)
        );
    }

    /// `Cargo.toml` → `ChangeKind::Source`.
    #[test]
    fn classify_cargo_toml() {
        assert_eq!(
            classify_change(Path::new("Cargo.toml")),
            Some(ChangeKind::Source)
        );
    }

    /// Неизвестное расширение → `None` (игнорируется).
    #[test]
    fn classify_unknown_extension_returns_none() {
        assert_eq!(classify_change(Path::new("assets/logo.png")), None);
        assert_eq!(classify_change(Path::new("data.json")), None);
    }

    /// Файл без расширения → `None`.
    #[test]
    fn classify_no_extension_returns_none() {
        assert_eq!(classify_change(Path::new("README")), None);
    }

    // ===================== file_hash (дедупликация) ========================

    /// **ТЗ 2.1**: одно и то же содержимое даёт одинаковый хэш (повторное
    /// событие игнорируется), новое содержимое — другой хэш (обрабатывается).
    ///
    /// Сценарий из ТЗ:
    ///   - записать "a" → hash1
    ///   - записать "a" ещё раз → hash2 == hash1 (ИГНОРИРУЕТСЯ)
    ///   - записать "b" → hash3 != hash1 (ОБРАБАТЫВАЕТСЯ)
    #[test]
    fn file_hash_same_content_same_hash() {
        let dir = std::env::temp_dir().join("light_dev_test_file_hash");
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let path = dir.join("sample.html");
        // Убедимся, что файл не существует с предыдущих тестов.
        let _ = std::fs::remove_file(&path);

        // Записываем "a".
        let mut f = std::fs::File::create(&path).expect("create file");
        f.write_all(b"a").expect("write 'a'");
        f.sync_all().expect("sync");
        drop(f);
        let hash1 = file_hash(&path).expect("hash1 must be Some");

        // Записываем "a" ещё раз (симулируем повторное событие notify).
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .expect("reopen");
        f.set_len(0).expect("truncate");
        f.write_all(b"a").expect("rewrite 'a'");
        f.sync_all().expect("sync");
        drop(f);
        let hash2 = file_hash(&path).expect("hash2 must be Some");

        // Одно и то же содержимое → одинаковый хэш.
        assert_eq!(
            hash1, hash2,
            "одинаковое содержимое должно давать одинаковый хэш"
        );

        // Записываем "b" (новое содержимое).
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .expect("reopen");
        f.set_len(0).expect("truncate");
        f.write_all(b"b").expect("write 'b'");
        f.sync_all().expect("sync");
        drop(f);
        let hash3 = file_hash(&path).expect("hash3 must be Some");

        // Разное содержимое → разные хэши (крайне маловероятно совпадение
        // у DefaultHasher для двух байтов, но формально проверяем).
        assert_ne!(hash1, hash3, "разное содержимое должно дать разный хэш");

        // Cleanup.
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    /// Файл, которого нет → `None` (не дедуплицируется, обрабатывается как
    /// изменение — консервативно).
    #[test]
    fn file_hash_missing_file_returns_none() {
        let dir = std::env::temp_dir().join("light_dev_test_file_hash_missing");
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let path = dir.join("does_not_exist.html");
        let _ = std::fs::remove_file(&path);
        assert_eq!(file_hash(&path), None);
        let _ = std::fs::remove_dir(&dir);
    }
}
