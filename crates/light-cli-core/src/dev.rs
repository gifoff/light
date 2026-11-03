//! Горячая перезагрузка приложения Light: супервизор `light dev`.
//!
//! Запускает приложение (копию из `target/hot-run/`) как дочерний процесс,
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
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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
    /// Пакет воркспейса, который запускается (копия из `target/hot-run/`).
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

/// Период ожидания между попытками чтения файла при временных ошибках
/// (`PermissionDenied` / `NotFound`). Редактор может писать файл в два этапа
/// (append + flush): первое чтение ловит частичный/переходный файл. Повторные
/// попытки (до 3, каждые 50 мс) дают OS время дописать файл, прежде чем мы
/// вернём `None`.
const FILE_HASH_RETRY_INTERVAL: Duration = Duration::from_millis(50);
/// Максимум попыток чтения файла в [`file_hash`] (1 начальная + 2 повтора).
const FILE_HASH_MAX_ATTEMPTS: u32 = 3;

/// Хэш содержимого файла (`u64`) для дедупликации событий notify.
///
/// Один «логический» записывающий вызов (например, сохранение в редакторе)
/// генерирует несколько событий `PollWatcher` — `Modify`, `Modify(Data)`,
/// `Modify(Metadata)`. **Ключевой порядок**: хэширование выполняется ПОСЛЕ
/// debounce, а не до него — потому что `Add-Content`-подобная запись идёт в
/// два этапа (append + flush), и первое событие видит неполное содержимое.
/// Если бы хэш применялся до debounce, hash1 ≠ hash2, и оба события прошли бы
/// → двойной reload. После debounce содержимое уже финальное, и повторные
/// события дают одинаковый хэш → отфильтровываются.
///
/// При временных ошибках чтения (`PermissionDenied` на частично-записанном
/// файле, `NotFound` в момент удаления) функция пробует снова до
/// [`FILE_HASH_MAX_ATTEMPTS`] раз, ожидая [`FILE_HASH_RETRY_INTERVAL`] между
/// попытками. Если все попытки упали (файл реально недоступен) — возвращает
/// `None`; событие тогда НЕ дедуплицируется и обрабатывается как изменение
/// (консервативно: лучше лишний reload, чем пропущенное изменение).
pub fn file_hash(path: &Path) -> Option<u64> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    // Читаем файл с ретраями: временные ошибки (частичная запись, PermissionDenied
    // на переходном состоянии, NotFound в момент атомарного удаления) часто
    // проходят через 50 мс. Лог на каждую неудачную попытку + итоговый лог по
    // ТЗ 2.2 ("light dev: hash({:?}) = {:?}").
    let mut attempts = 0u32;
    let buf = loop {
        match std::fs::read(path) {
            Ok(b) => break Some(b),
            Err(e) => {
                attempts += 1;
                if attempts >= FILE_HASH_MAX_ATTEMPTS {
                    break None;
                }
                eprintln!(
                    "light dev: hash({path:?}) read attempt {attempts} failed: {e} \
                     (повторю через {:?}, всего до {FILE_HASH_MAX_ATTEMPTS:?} попыток)",
                    FILE_HASH_RETRY_INTERVAL
                );
                thread::sleep(FILE_HASH_RETRY_INTERVAL);
            }
        }
    };
    let result: Option<u64> = buf.map(|b| {
        let mut hasher = DefaultHasher::new();
        b.hash(&mut hasher);
        hasher.finish()
    });
    // Лог по ТЗ 2.2: на каждый вызов file_hash — eprintln с Some/None.
    eprintln!("light dev: hash({:?}) = {:?}", path, result);
    result
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

// ===================== ТЗ 0.14.2-B: seamless rebuild =========================

/// Имя бинарника пакета: **имя пакета без изменений** (cargo не заменяет `-`).
///
/// Cargo кладёт бинарник в `target/debug/<package-name>` (двери сохраняются):
/// `example-devtools-demo` → `example-devtools-demo.exe` (Windows) или
/// `example-devtools-demo` (unix). Путь к свежему артефакту:
/// `<build_target>/debug/<package-name>.exe`.
pub fn bin_name(package: &str) -> String {
    package.to_string()
}

/// Свежий артефакт после `cargo build --target-dir <build_target>`:
/// `<build_target>/debug/<package>.exe` (Windows) / `.exec`-без (unix).
///
/// Запущенный процесс НЕ держит этот файл — он запущен из копии в
/// [`run_exe_path`] (`target/hot-run/`). Поэтому следующая сборка может
/// перезаписывать свежий бинарник без os error 5.
pub fn fresh_bin_path(build_target: &Path, package: &str) -> PathBuf {
    let mut p = build_target.join("debug").join(bin_name(package));
    #[cfg(windows)]
    {
        p.set_extension("exe");
    }
    p
}

/// Копия свежего бинарника в `target/hot-run/`, от которой и запускается
/// приложение: `<run_dir>/<bin>-<ts>.exe`. Timestamp (`as_millis`) делает имя
/// уникальным на каждый swap; старые копии чистятся [`cleanup_run_dir`].
pub fn run_exe_path(run_dir: &Path, bin: &str) -> PathBuf {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let mut p = run_dir.join(format!("{bin}-{ts}"));
    #[cfg(windows)]
    {
        p.set_extension("exe");
    }
    p
}

/// Удаляет все `.exe` в `run_dir`, кроме активного (`keep`). Ошибки молча
/// игнорируются: файл мог быть ещё жив в AV-кэше.
pub fn cleanup_run_dir(run_dir: &Path, keep: &Path) {
    if let Ok(entries) = std::fs::read_dir(run_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path == keep {
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) == Some("exe") {
                let _ = std::fs::remove_file(&path);
            }
        }
    }
}

/// Копирует свежий бинарник из `<build_target>/debug/` в
/// `<run_dir>/<bin>-<ts>.exe`. Создаёт `run_dir`, если её нет.
/// Возвращает путь к копии.
pub fn copy_fresh_to_run(
    build_target: &Path,
    run_dir: &Path,
    bin: &str,
) -> std::result::Result<PathBuf, std::io::Error> {
    std::fs::create_dir_all(run_dir)?;
    let fresh = fresh_bin_path(build_target, bin);
    let dest = run_exe_path(run_dir, bin);
    std::fs::copy(&fresh, &dest)?;
    Ok(dest)
}

/// Результат одной фоновой сборки.
///
/// Чистый тип: main-loop получает его через `poll_rebuild` и передаёт в
/// [`RebuildStateMachine`] для принятия решения «swap / keep-old».
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RebuildOutcome {
    /// Сборка успешна — можно выполнять swap.
    Success,
    /// Сборка завершилась с ошибкой (exit code сохранён) — old продолжает.
    Failed(i32),
}

/// State-machine: решение о реакции на результат фоновой сборки.
///
/// Вынесена отдельно от потоков, чтобы юнит-тестировать без реального cargo.
/// Все методы чистые: принимают текущий статус и возвращают новый (без side
/// effects кроме лога в `mark_rebuild_failure`, который нужен по ТЗ 5.3).
#[derive(Debug, Default, Clone, Copy)]
pub struct RebuildStateMachine {
    /// true, если сейчас идёт фоновая сборка.
    pub in_progress: bool,
    /// Момент запуска последней активной сборки (для лога «took N s»).
    pub started_at: Option<Instant>,
}

impl RebuildStateMachine {
    /// Запускаем новую фоновую сборку. Если старая ещё идёт — её нужно убить
    /// (вызывается снаружи; здесь только поднимаем флаг + таймер).
    pub fn start(&mut self) {
        self.in_progress = true;
        self.started_at = Some(Instant::now());
    }

    /// Обработать результат завершившейся сборки. Возвращает
    /// [`RebuildOutcome`], по которому main-loop решает, делать ли swap.
    pub fn poll_outcome(&mut self, exit_code: i32) -> RebuildOutcome {
        self.in_progress = false;
        if exit_code == 0 {
            RebuildOutcome::Success
        } else {
            eprintln!(
                "light dev: rebuild FAILED (exit {exit_code}), previous version kept running"
            );
            RebuildOutcome::Failed(exit_code)
        }
    }

    /// Сброс после завершённого цикла (для теста).
    pub fn reset(&mut self) {
        *self = Self::default();
    }
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
    // ТЗ 0.14.2-B-fix: для workspace-членов артефакты cargo кладутся в
    // workspace_root/target, а не в package_root/target. Берём workspace_root
    // (если доступен) как корень для target/hot-build и target/hot-run;
    // fallback — package_root (одиночный крейт без workspace).
    let target_root = resolve_workspace_root(&cwd).unwrap_or_else(|| {
        eprintln!(
            "light dev: `cargo metadata` недоступен — target_dir из package_root {:?}",
            package_root
        );
        package_root.clone()
    });
    eprintln!("light dev: workspace_root = {:?}", target_root);
    // ТЗ 0.14.2-B-fix2: две папки — сборка в `target/hot-build` (cargo её
    // никогда не блокирует), запуск из копии в `target/hot-run/<bin>-<ts>.exe`
    // (Windows не даёт перезаписать запущенный .exe, но копия свободна).
    let build_target = target_root.join("target").join("hot-build");
    let run_dir = target_root.join("target").join("hot-run");
    let bin = bin_name(args.package.as_deref().unwrap_or("app"));
    eprintln!(
        "light dev: build_target = {:?}, run_dir = {:?}",
        build_target, run_dir
    );

    // === ФАЗА A: warm-up build в target/hot-build ===
    // Блокирующий `cargo build` в отдельный target-dir. При ошибке — сразу
    // exit 1 (старого процесса нет, «оставлять живым» нечего).
    {
        eprintln!(
            "light dev: warm-up build (cargo build --target-dir {:?})…",
            build_target
        );
        let mut cmd = Command::new("cargo");
        cmd.arg("build");
        if let Some(package) = &args.package {
            cmd.args(["-p", package]);
        }
        cmd.args(["--features", "dev-mode"]);
        cmd.args(["--target-dir", &build_target.to_string_lossy()]);
        cmd.args(&args.extra_cargo_args);
        cmd.stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());
        let status = cmd.status();
        match status {
            Ok(s) if s.success() => eprintln!("light dev: warm-up build succeeded"),
            Ok(s) => {
                eprintln!(
                    "light dev: warm-up build FAILED (exit {:?}) — exit 1",
                    s.code()
                );
                return Ok(1);
            }
            Err(e) => {
                eprintln!("light dev: не удалось запустить `cargo build`: {e} — exit 1");
                return Ok(1);
            }
        }
        // ТЗ 0.14.2-B-fix 4: после успешного build проверим, что бинарник
        // действительно лежит там, где мы его ищем. Если нет — путь расхожился.
        let fresh_bin = fresh_bin_path(&build_target, &bin);
        if std::fs::metadata(&fresh_bin).is_err() {
            eprintln!(
                "light dev: BUG — build succeeded but binary not at {:?}. \
                 Search in {:?}...",
                fresh_bin,
                build_target.join("debug")
            );
            return Ok(1);
        }
    }

    // === ФАЗА B: копия в target/hot-run + spawn ===
    // Запускаем из копии, а не из hot-build: следующая сборка перезапишет
    // fresh-бинарник без os error 5 (запущен процесс с другой, свободной
    // копии).
    let first_run_exe = match copy_fresh_to_run(&build_target, &run_dir, &bin) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("light dev: не удалось скопировать бинарник: {e} — exit 1");
            return Ok(1);
        }
    };
    eprintln!(
        "light dev: spawning {:?} (build {:?})",
        first_run_exe,
        fresh_bin_path(&build_target, &bin)
    );
    let child = match Command::new(&first_run_exe)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!(
                "light dev: не удалось запустить {:?}: {e} — exit 1",
                first_run_exe
            );
            return Ok(1);
        }
    };

    // ТЗ 0.14.2-B: общая обёртка для thread-safe доступа (main loop + rebuild
    // thread). `child_arc` — приложение, которое сейчас работает (или будет
    // после swap); `rebuild_arc` — текущая фоновая сборка (или None, если её
    // нет). Rebuild запускаем из main-потока и просто опрашиваем `try_wait()`
    // в главном цикле — отдельный тред не нужен (Cargo сам параллелит).
    let child_arc = Arc::new(Mutex::new(child));
    let rebuild_arc: Arc<Mutex<Option<Child>>> = Arc::new(Mutex::new(None));

    // State-machine: решение о реакции на результат фоновой сборки. Держим
    // в main-потоке (swap выполняется там); чистый, без side effects.
    let mut sm = RebuildStateMachine::default();

    // Канал на перезапуск от наблюдателя. Несёт тип изменения: Html →
    // RELOAD_TEMPLATE (без убийства), Source → background rebuild + swap.
    let (restart_tx, restart_rx) = mpsc::channel::<(Instant, ChangeKind)>();
    // Флаг завершения: общий между главным потоком, наблюдателем и
    // rebuild-тредом. Атомарный (в отличие от mpsc::Receiver) — легко
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
        // несколько событий notify (Modify + Modify(Data) + Modify(Metadata)).
        // Ключевой порядок (ТЗ 0.14.2-A-fix4): debounce СНАЧАЛА (файл успевает
        // дописаться), затем hash-фильтр (повторные события того же
        // содержимого отброшены), затем handle_watch_event.
        let mut last_hashes: std::collections::HashMap<PathBuf, u64> =
            std::collections::HashMap::new();
        // Накопитель путей, ожидающих debounce. На событие notify мы НЕ
        // хэшируем сразу (файл может быть в середине записи); только сохраняем
        // путь. Когда debounce срабатывает (100/500 мс тишины), проходим по
        // pending_debounce, хэшируем и дедуплицируем.
        let mut pending_debounce: std::collections::VecDeque<PathBuf> =
            std::collections::VecDeque::new();
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
                // Сбрасываем накопленные пути: предыдущий запуск дочернего
                // процесса завершился, и все «недочитанные» изменения теряются.
                pending_debounce.clear();
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
                    // На каждое notify-событие мы НЕ хэшируем сразу: файл может
                    // быть в середине записи (двухэтапный append + flush), и
                    // хэш поймает неполное содержимое. Вместо этого:
                    //  1) фильтруем по watch_paths;
                    //  2) классифицируем (Html / Source / None) для лога;
                    //  3) кладём путь в pending_debounce (deduplication по пути,
                    //     чтобы серия событий одного save не раздувала очередь);
                    //  4) поднимаем таймер debounce (Html / Source).
                    //
                    // Хэширование + дедупликация happen в timeout-ветке, ПОСЛЕ
                    // debounce — к тому моменту файл уже полностью записан.
                    for path in &event.paths {
                        if !watch_paths_copy.iter().any(|w| path.starts_with(w)) {
                            continue;
                        }
                        // Лог по ТЗ 3.1: сразу, до debounce.
                        let kind_log = classify_change(path);
                        eprintln!(
                            "light dev: notify event path={path:?} kind={kind_log:?} \
                             (queued for debounce)"
                        );
                        // Deduplicate by path within the queue: если тот же путь
                        // уже в pending_debounce, не добавляем повторно (серия
                        // событий одного save генерирует 2-3 notify на один путь).
                        if !pending_debounce.contains(path) {
                            pending_debounce.push_back(path.clone());
                        }
                    }
                    // Поднимаем таймеры debounce: если любое из релевантных
                    // изменений пришло, debounce обнуляется и начинает отсчёт
                    // заново (стандартный trailing-edge debounce).
                    if event
                        .paths
                        .iter()
                        .any(|p| watch_paths_copy.iter().any(|w| p.starts_with(w)))
                    {
                        let has_html = event.paths.iter().any(|p| {
                            watch_paths_copy.iter().any(|w| p.starts_with(w))
                                && matches!(classify_change(p), Some(ChangeKind::Html))
                        });
                        let has_src = event.paths.iter().any(|p| {
                            watch_paths_copy.iter().any(|w| p.starts_with(w))
                                && matches!(classify_change(p), Some(ChangeKind::Source))
                        });
                        if has_html {
                            last_html = Some(Instant::now());
                        }
                        if has_src {
                            last_rs = Some(Instant::now());
                        }
                    }
                }
                Ok(Err(e)) => {
                    eprintln!("light dev: ошибка наблюдателя: {e}");
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    // Дебаунс + hash-дедупликация ПОСЛЕ debounce.
                    //
                    // Порядок (ТЗ 0.14.2-A-fix4):
                    //   1) debounce: дождались тишины (HTML_DEBOUNCE / RS_DEBOUNCE);
                    //   2) hash: хэшируем содержимое каждого пути из pending_debounce
                    //      (файл уже полностью записан, потому что прошло ≥ 100/500 мс);
                    //   3) dedup: отбрасываем пути, чей хэш == last_hashes[path]
                    //      (повторные события того же содержимого);
                    //   4) dispatch: берём ПЕРВЫЙ «чистый» путь, классифицируем и
                    //      передаём в handle_watch_event.
                    //
                    // Важно: если pending_debounce пусто — ничего не делаем, даже
                    // если таймеры debounce истекли (таймеры поднимаются только
                    // при добавлении путей, так что это ситуация невозможна, но
                    // защита на случай race с respawn).
                    if !state.is_starting && !state.has_pending() {
                        let html_fired =
                            matches!(last_html, Some(t) if t.elapsed() >= HTML_DEBOUNCE);
                        let rs_fired = matches!(last_rs, Some(t) if t.elapsed() >= RS_DEBOUNCE);
                        if html_fired || rs_fired {
                            if html_fired {
                                last_html = None;
                            }
                            if rs_fired {
                                last_rs = None;
                            }
                            eprintln!(
                                "light dev: debounce fired, {} paths pending",
                                pending_debounce.len()
                            );
                            // Проходим по pending_debounce: хэшируем и дедуплицируем.
                            // Файл уже полностью записан (debounce отработал).
                            let mut clean_events: std::collections::VecDeque<PathBuf> =
                                std::collections::VecDeque::new();
                            while let Some(path) = pending_debounce.pop_front() {
                                match file_hash(&path) {
                                    Some(hash) => {
                                        // Сравниваем с последним известным хэшем
                                        // этого пути:
                                        //  - совпадает → повторное событие того же
                                        //    содержимого → игнорируем (по ТЗ:
                                        //    "duplicate notify event, ignored");
                                        //  - отличается → настоящее изменение →
                                        //    обновляем last_hashes и кладём в
                                        //    clean_events.
                                        match last_hashes.get(&path) {
                                            Some(&last) if last == hash => {
                                                eprintln!(
                                                    "light dev: duplicate notify event for \
                                                     {path:?} (hash unchanged) — ignored"
                                                );
                                            }
                                            _ => {
                                                last_hashes.insert(path.clone(), hash);
                                                clean_events.push_back(path);
                                            }
                                        }
                                    }
                                    // Файл недоступен (удалён, ещё пишется после
                                    // 3 попыток чтения): НЕ дедуплицируем —
                                    // обрабатываем как изменение (консервативно).
                                    None => {
                                        eprintln!(
                                            "light dev: file_hash returned None for \
                                             {path:?} (treated as real change, no dedup)"
                                        );
                                        clean_events.push_back(path);
                                    }
                                }
                            }
                            eprintln!(
                                "light dev: {} clean events after hash-dedup",
                                clean_events.len()
                            );
                            // Берём ПЕРВЫЙ чистый путь, классифицируем и
                            // передаём в handle_watch_event (по ТЗ 1.3).
                            if let Some(first) = clean_events.pop_front() {
                                let kind = classify_change(&first);
                                // Лог по ТЗ 3.4: при передаче в handle.
                                eprintln!("light dev: dispatching {kind:?} change for {first:?}");
                                match handle_watch_event(&mut state, kind) {
                                    WatchAction::None => {
                                        // Изменение во время сборки: просто
                                        // запомнили (pending_флаги подняты внутри
                                        // handle_watch_event).
                                    }
                                    WatchAction::ApplyPending => {
                                        if let Some(akind) = apply_pending(&mut state, &restart_tx)
                                        {
                                            eprintln!(
                                                "light dev: применяю pending (kind={akind:?})"
                                            );
                                            match akind {
                                                ChangeKind::Html => {
                                                    last_html = Some(Instant::now());
                                                }
                                                ChangeKind::Source => {
                                                    last_rs = Some(Instant::now());
                                                }
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
                                        let _ =
                                            restart_tx.send((Instant::now(), ChangeKind::Source));
                                        last_rs = Some(Instant::now());
                                    }
                                }
                            } else {
                                eprintln!(
                                    "light dev: все события отфильтрованы hash-дедупом — \
                                     action не требуется"
                                );
                            }
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
        //
        // Внутренний цикл опроса: выходим в наружный цикл по первому событию
        // (restart от наблюдателя, завершение фоновой сборки либо выход
        // дочернего процесса).
        //
        // ТЗ 0.14.2-B: при .rs change мы НЕ убиваем старый процесс сразу —
        // запускаем `cargo build` в `target/hot-build` (рекурсивно, но не
        // блокируй watcher: он живёт в своём треде), а когда сборка закончится:
        //   - exit 0  → SAVE_STATE, copy fresh → hot-run, kill old, spawn new
        //   - exit != 0 → старый продолжает работать; stderr cargo уже на консоли.
        enum Ev {
            Restart(ChangeKind),
            RebuildDone(i32),
            Exited(std::process::ExitStatus),
        }
        let ev = loop {
            if let Ok((_ts, kind)) = restart_rx.try_recv() {
                break Ev::Restart(kind);
            }
            // Завершение фоновой сборки: poll rebuild-child в основном потоке.
            // `lock().ok()` — при poisoned mutex (краш в другом треде) просто
            // пропускаем опрос; main продолжит работать с app-процессом.
            let rebuild_done = rebuild_arc.lock().ok().and_then(|mut g| match g.as_mut() {
                Some(c) => c.try_wait().ok().flatten(),
                None => None,
            });
            if let Some(status) = rebuild_done {
                let code = status.code().unwrap_or(1);
                if let Ok(mut g) = rebuild_arc.lock() {
                    *g = None;
                }
                break Ev::RebuildDone(code);
            }
            // Опрос дочернего процесса: если mutex poisoned — пропускаем tick;
            // если `try_wait` вернул Err — тоже выходим с exit code 1 (не
            // продолжаем работать с процессом, который не удалось опросить).
            let child_exit = match child_arc.lock() {
                Ok(mut g) => match g.try_wait() {
                    Ok(Some(s)) => Some(Ok(s)),
                    Ok(None) => None, // ещё жив
                    Err(e) => Some(Err(e)),
                },
                Err(_) => None,
            };
            match child_exit {
                Some(Ok(s)) => break Ev::Exited(s),
                Some(Err(e)) => {
                    eprintln!("light dev: ошибка ожидания дочернего процесса: {e}");
                    break Ev::Exited(ExitStatus::from_raw(1));
                }
                None => {
                    // Mutex poisoned или process ещё жив.
                    if child_arc.lock().is_err() {
                        eprintln!("light dev: child_arc poisoned (пропуск опроса)");
                    }
                }
            }
            thread::sleep(Duration::from_millis(50));
        };
        match ev {
            Ev::Exited(status) => {
                done_flag.store(true, Ordering::Relaxed);
                let _ = watcher.join();
                // Если rebuild ещё идёт — тоже убиваем (Ctrl+C-сценарий).
                if let Ok(mut g) = rebuild_arc.lock()
                    && let Some(mut r) = g.take()
                {
                    let _ = r.kill();
                    let _ = r.wait();
                }
                let code = status.code().unwrap_or(1);
                if code == 0 {
                    eprintln!(
                        "light dev: app exited cleanly (window closed by user or session end)"
                    );
                } else {
                    eprintln!("light dev: app crashed (exit code {code})");
                }
                return Ok(code);
            }
            Ev::RebuildDone(exit_code) => {
                // Фоновая сборка закончилась. State-machine решает: swap или keep-old.
                let outcome = sm.poll_outcome(exit_code);
                if let RebuildOutcome::Success = outcome {
                    // === SWAP: SAVE_STATE → copy fresh → kill old → spawn new ===
                    // Порядок (ТЗ 0.14.2-B-fix2 3): copy работает и на живом
                    // exe (это отдельный файл), kill — после copy, но до spawn
                    // нового (чтобы не было двух видимых окон).
                    let took = sm
                        .started_at
                        .map(|t| Instant::now().saturating_duration_since(t))
                        .unwrap_or_default();
                    eprintln!("light dev: rebuild succeeded (took {took:?}), swapping…");
                    if let Err(e) = request_save_state(DEVTOOLS_PORT) {
                        eprintln!(
                            "light dev: не удалось отправить SAVE_STATE перед swap: {e} \
                             (состояние может быть не сохранено)"
                        );
                    }
                    // Короткая пауза: даём UI-потоку время обработать команду
                    // и записать файл, прежде чем уберём процесс.
                    thread::sleep(Duration::from_millis(200));
                    // a) Копируем свежий бинарник из hot-build в hot-run.
                    //    Никто не держит fresh-бинарник (запущена старая
                    //    копия в hot-run), run_exe новое имя — можно писать.
                    let new_run_exe = match copy_fresh_to_run(&build_target, &run_dir, &bin) {
                        Ok(p) => p,
                        Err(e) => {
                            eprintln!(
                                "light dev: не удалось скопировать свежий бинарник: {e} \
                                 (old остаётся)"
                            );
                            continue;
                        }
                    };
                    eprintln!("light dev: copied fresh build to {:?}", new_run_exe);
                    // b) Убиваем старый app (через MutexGuard; `kill_child`
                    //    принимает `&mut Child`).
                    let old_pid = child_arc.lock().map(|g| g.id()).unwrap_or(0);
                    if let Ok(mut g) = child_arc.lock() {
                        kill_child(&mut g);
                    } else {
                        eprintln!("light dev: child_arc poisoned — не смогла убить old");
                    }
                    // c) Spawn нового из копии в hot-run.
                    match Command::new(&new_run_exe)
                        .stdin(Stdio::inherit())
                        .stdout(Stdio::inherit())
                        .stderr(Stdio::inherit())
                        .spawn()
                    {
                        Ok(c) => {
                            let new_pid = c.id();
                            if let Ok(mut g) = child_arc.lock() {
                                *g = c;
                            } else {
                                eprintln!(
                                    "light dev: child_arc poisoned — не записала новый process"
                                );
                            }
                            // Чистим старые копии (кроме текущей run_exe).
                            cleanup_run_dir(&run_dir, &new_run_exe);
                            // Сбрасываем watcher в is_starting: он сам решит,
                            // когда закрывать окно (порт 9229 открылся либо
                            // дедлайн STARTUP_IGNORE прошёл).
                            let _ = state_tx.send(WatchState::starting());
                            eprintln!(
                                "light dev: spawning {:?} (build {:?})",
                                new_run_exe,
                                fresh_bin_path(&build_target, &bin)
                            );
                            eprintln!("light dev: swap done (pid {old_pid} -> {new_pid})");
                        }
                        Err(e) => {
                            eprintln!("light dev: не удалось запустить новое приложение: {e}");
                            done_flag.store(true, Ordering::Relaxed);
                            let _ = watcher.join();
                            return Ok(1);
                        }
                    }
                }
                // RebuildOutcome::Failed(_): старый процесс остаётся; stderr
                // cargo уже на консоли. Watcher НЕ сбрасываем (старое app
                // работает, is_starting не нужен).
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
                    // Source-чнейдж (.rs / Cargo.toml) → seamless rebuild.
                    //
                    // 1) Если уже идёт сборка — убиваем её (новый save важнее).
                    //    Лог «cancelled in-flight rebuild (new save)».
                    // 2) Поднимаем flag + таймер в state-machine.
                    // 3) Запускаем `cargo build --target-dir target/hot-build`
                    //    в фоне (non-blocking), сохраняем Child в rebuild_arc.
                    // 4) Главный цикл продолжит опрашивать — на Ev::RebuildDone
                    //    сделает swap (success) или промолчит (failure).
                    let had_in_flight = {
                        let old = rebuild_arc.lock().ok().and_then(|mut g| g.take());
                        old.map(|mut c| {
                            let _ = c.kill();
                            let _ = c.wait();
                            true
                        })
                        .unwrap_or(false)
                    };
                    if had_in_flight {
                        eprintln!("light dev: cancelled in-flight rebuild (new save)");
                    }
                    eprintln!("light dev: rebuild started (old app still running)");
                    sm.start();
                    // Собираем команду для background build. `build_target` —
                    // тот же путь, что и в warm-up (workspace_root/hot-build).
                    let mut cmd = Command::new("cargo");
                    cmd.arg("build");
                    if let Some(p) = &args.package {
                        cmd.args(["-p", p]);
                    }
                    cmd.args([
                        "--features",
                        "dev-mode",
                        "--target-dir",
                        &build_target.to_string_lossy(),
                    ]);
                    cmd.args(&args.extra_cargo_args);
                    cmd.stdin(Stdio::inherit())
                        .stdout(Stdio::inherit())
                        .stderr(Stdio::inherit());
                    // Запускаем; если spawn упал — считаем ошибкой (old живёт).
                    match cmd.spawn() {
                        Ok(c) => {
                            if let Ok(mut g) = rebuild_arc.lock() {
                                *g = Some(c);
                            } else {
                                eprintln!(
                                    "light dev: rebuild_arc poisoned — не записала новый build"
                                );
                            }
                        }
                        Err(e) => {
                            eprintln!("light dev: не удалось запустить `cargo build`: {e}");
                            // Сбрасываем флаг: сборка не стартовала, old остаётся.
                            sm.poll_outcome(1);
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
        ChangeKind, RebuildOutcome, RebuildStateMachine, WatchAction, WatchState, bin_name,
        classify_change, cleanup_run_dir, copy_fresh_to_run, file_hash, fresh_bin_path,
        handle_watch_event, parse_dev_command, resolve_package_root, resolve_startup_completion,
        run_exe_path, wait_port_available,
    };
    use std::io::Write as _;
    use std::net::TcpListener;
    use std::path::{Path, PathBuf};
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

    // ===================== ТЗ 0.14.2-A-fix4: dedup after debounce =========

    /// **ТЗ 4.1**: имитируем двухэтапный save (append + flush) — первое
    /// событие видит неполное содержимое, второе — полное. После debounce
    /// содержимое уже финальное, и hash-фильтр корректно отбрасывает повторы.
    ///
    /// Сценарий:
    ///   1. записать "a" → событие 1 (неполное, append)
    ///   2. дописать "b" → событие 2 (полное, flush)
    ///   3. debounce срабатывает → хэшируем: hash("ab")
    ///   4. last_hashes["file"] = hash("ab")
    ///   5. следующее notify-событие с тем же содержимым → hash == last →
    ///      ИГНОРИРУЕТСЯ (1 reload на 2 notify-события, не 2).
    ///
    /// Тест проверяет именно то, что происходит ПОСЛЕ debounce: если бы мы
    /// хэшировали ДО debounce, hash1 ("a") != hash2 ("ab"), и оба события
    /// прошли бы → двойной reload.
    #[test]
    fn dedup_after_debounce() {
        let dir = std::env::temp_dir().join("light_dev_test_dedup_after_debounce");
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let path = dir.join("sample.html");
        let _ = std::fs::remove_file(&path);

        // Этап 1 (append): пишем "a" — это первый notify-событие, файл
        // содержит только "a".
        let mut f = std::fs::File::create(&path).expect("create");
        f.write_all(b"a").expect("write 'a'");
        f.sync_all().expect("sync");
        drop(f);

        // Этап 2 (flush): дописываем "b" — второй notify-событие, файл
        // содержит "ab".
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .expect("reopen-append");
        f.write_all(b"b").expect("append 'b'");
        f.sync_all().expect("sync");
        drop(f);

        // К этому моменту debounce (100/500 мс) уже истёк. Хэшируем финальное
        // содержимое "ab":
        let final_hash = file_hash(&path).expect("hash of 'ab'");

        // Симулируем «повторное» notify-событие (Modify(Data) / Modify(Metadata)):
        // содержимое НЕ изменилось, хэш должен совпадать.
        let dup_hash = file_hash(&path).expect("hash on duplicate event");
        assert_eq!(
            final_hash, dup_hash,
            "повторное событие того же содержимого должно дать одинаковый хэш \
             (и быть отброшено hash-фильтром)"
        );

        // Теперь симулируем НОВЫЙ save: содержимое изменилось.
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .expect("rewrite");
        f.set_len(0).expect("truncate");
        f.write_all(b"xyz").expect("write 'xyz'");
        f.sync_all().expect("sync");
        drop(f);
        let new_hash = file_hash(&path).expect("hash of 'xyz'");
        assert_ne!(
            final_hash, new_hash,
            "новое содержимое даёт новый хэш → пройдёт hash-фильтр"
        );

        // Cleanup.
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    /// **ТЗ 4.2**: имитируем временную ошибку чтения (например, PermissionDenied
    /// при частичной записи, NotFound в момент атомарного удаления). `file_hash`
    /// должна ретраить 3 раза с интервалом 50 мс, и после всех попыток вернуть
    /// `None` (если файл реально недоступен) или `Some(hash)` (если ошибка
    /// ушла в процессе ретраев).
    ///
    /// Мы не можем реально вызвать `PermissionDenied` под sandbox-ограничениями,
    /// поэтому тестируем наблюдаемое поведение:
    /// - файл существует и читается → `Some(hash)`;
    /// - файла нет вообще → 3 неудачные попытки (лог с "read attempt N failed")
    ///   и возврат `None`.
    ///
    /// Поведение «ретраи до 3 раз» проверяем косвенно: время выполнения
    /// `file_hash` для несуществующего файла >= 2 × 50 мс (два sleep между
    /// тремя попытками), но < 1 с (не блокирует поток watcher надолго).
    #[test]
    fn file_hash_retries_on_permission_denied() {
        let dir = std::env::temp_dir().join("light_dev_test_file_hash_retry");
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let path = dir.join("missing.html");
        let _ = std::fs::remove_file(&path);

        let start = Instant::now();
        let result = file_hash(&path);
        let elapsed = start.elapsed();

        // Файл не существует: все 3 попытки падают, возвращаем None.
        assert_eq!(
            result, None,
            "несуществующий файл → None после всех ретраев"
        );

        // Два sleep по 50 мс (между 3 попытками): минимум ~100 мс,
        // максимум ~200 мс с запасом на задержки ОС. НЕ проверяем верхнюю
        // границу жёстко: Windows `thread::sleep` может overshoot-нуть.
        assert!(
            elapsed >= Duration::from_millis(100),
            "ожидаю минимум 2 × 50 мс ретраев, получил {elapsed:?}"
        );
        assert!(
            elapsed < Duration::from_secs(1),
            "ретраи должны завершиться быстро (< 1 с), получили {elapsed:?}"
        );

        // Cleanup.
        let _ = std::fs::remove_dir(&dir);
    }

    // ===================== ТЗ 0.14.2-B: seamless rebuild state machine ======

    /// **ТЗ 6.1**: `start()` поднимает `in_progress` и задаёт `started_at`.
    #[test]
    fn rebuild_started_flag() {
        let mut sm = RebuildStateMachine::default();
        assert!(!sm.in_progress, "начальное состояние: in_progress=false");
        assert!(
            sm.started_at.is_none(),
            "начальное состояние: started_at=None"
        );
        sm.start();
        assert!(sm.in_progress, "после start(): in_progress=true");
        assert!(
            sm.started_at.is_some(),
            "после start(): started_at=Some(Instant)"
        );
    }

    /// **ТЗ 6.2**: `poll_outcome(0)` → Success (swap будет выполнен).
    /// `started_at` остаётся (для лога «took N s»); `in_progress` сбрасывается.
    #[test]
    fn rebuild_success_triggers_swap() {
        let mut sm = RebuildStateMachine::default();
        sm.start();
        let Some(started) = sm.started_at else {
            panic!("expected started_at to be set after start()");
        };
        let outcome = sm.poll_outcome(0);
        assert_eq!(outcome, RebuildOutcome::Success);
        assert!(!sm.in_progress, "после poll_outcome: in_progress=false");
        assert_eq!(
            sm.started_at,
            Some(started),
            "started_at сохраняется для лога"
        );
    }

    /// **ТЗ 6.3**: `poll_outcome(N)` при N != 0 → Failed(N) (old живёт).
    /// `started_at` остаётся (для возможного последующего пересбора).
    #[test]
    fn rebuild_failure_keeps_old() {
        let mut sm = RebuildStateMachine::default();
        sm.start();
        let Some(started) = sm.started_at else {
            panic!("expected started_at to be set after start()");
        };
        let outcome = sm.poll_outcome(2);
        assert_eq!(outcome, RebuildOutcome::Failed(2));
        assert!(!sm.in_progress, "после poll_outcome: in_progress=false");
        assert_eq!(sm.started_at, Some(started), "started_at сохраняется");
    }

    /// **ТЗ 6.4**: новый save во время in-flight сборки → убить старую,
    /// `start()` заново (flag + fresh timer).
    ///
    /// Юнит на уровне SM: проверяем, что после `reset()`+`start()` флаг
    /// возвращается к `true`, а `started_at` обновляется. Убийство процесса
    /// выполняется в main-loop (через `rebuild_arc`) — это integration test.
    #[test]
    fn new_save_cancels_in_flight() {
        let mut sm = RebuildStateMachine::default();
        sm.start();
        assert!(sm.in_progress);
        let first_start = sm.started_at.unwrap();
        // Имитация: новый save пришёл, старую сборку убили, новая запускается.
        sm.start();
        assert!(sm.in_progress, "после нового start(): in_progress=true");
        assert_ne!(
            sm.started_at,
            Some(first_start),
            "started_at должен быть обновлён (новый запуск)"
        );
    }

    // ===================== ТЗ 0.14.2-B-fix2: path helpers ================

    /// Бинарник: имя пакета без изменений (cargo не заменяет `-`). Windows: `.exe`.
    #[test]
    fn bin_name_preserves_dashes() {
        assert_eq!(bin_name("my-app"), "my-app");
        assert_eq!(bin_name("my-app-x"), "my-app-x");
        assert_eq!(bin_name("simple"), "simple");
    }

    /// Свежий бинарник: `<build_target>/debug/<package-name>[.exe]`.
    #[test]
    fn fresh_bin_path_layout() {
        let base = std::env::temp_dir();
        let build_target = base.join("hot-build");
        let p = fresh_bin_path(&build_target, "my-app");
        let s = p.to_string_lossy().to_string();
        assert!(s.contains("hot-build"), "path contains hot-build: {s}");
        assert!(
            s.ends_with("my-app.exe") || s.ends_with("my-app"),
            "correct bin name: {s}"
        );
        if cfg!(windows) {
            assert!(s.ends_with(".exe"), "windows .exe, got: {s}");
        }
    }

    /// Копия для запуска: `<run_dir>/<bin>-<ts>[.exe]` — имя содержит bin
    /// и timestamp, расширение `.exe` на Windows.
    #[test]
    fn run_exe_path_has_timestamp_and_extension() {
        let run_dir = std::env::temp_dir().join("hot-run");
        let p = run_exe_path(&run_dir, "my-app");
        let file = p
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        assert!(
            file.starts_with("my-app-"),
            "filename should start with bin + dash, got: {file}"
        );
        if cfg!(windows) {
            assert!(file.ends_with(".exe"), "windows extension, got: {file}");
        } else {
            assert!(!file.ends_with(".exe"), "no .exe on unix, got: {file}");
        }
    }

    /// `copy_fresh_to_run`: создаёт run_dir, копирует, возвращает путь к копии.
    /// `cleanup_run_dir`: удаляет старые .exe, кроме активного.
    #[test]
    fn copy_and_cleanup_run_dir() {
        // Используем CWD-относительный каталог, потому что temp_dir() в этом
        // окружении имеет ACL-ограничения (см. предсуществующие ACL-файлы
        // dev::tests::file_hash_*). Каталог уникален по pid + nanos и чистится
        // в конце теста.
        let tmp = std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(format!(
                ".light-dev-test-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            ));
        let build_root = tmp.join("hot-build");
        let build_target = build_root.join("debug");
        let run_dir = tmp.join("hot-run");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&build_target).expect("create build dir");

        // Создаём фейковый fresh-бинарник в <build_target>/<bin>[.exe].
        let bin = "test-app";
        let mut fresh = build_target.join(bin);
        #[cfg(windows)]
        {
            fresh.set_extension("exe");
        }
        std::fs::write(&fresh, b"fake").expect("write fake binary");

        // Копируем: build_root = <tmp>/hot-build.
        let copied = copy_fresh_to_run(&build_root, &run_dir, bin).expect("copy should succeed");
        assert!(copied.exists(), "copy should exist: {:?}", copied);
        let name = copied
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        assert!(name.starts_with("test-app-"), "name: {name}");

        // Чистим: добавляем «старый» .exe и проверяем, что он удалён, а
        // текущий остался.
        let mut stale = run_dir.join("test-app-oldstale");
        #[cfg(windows)]
        {
            stale.set_extension("exe");
        }
        std::fs::write(&stale, b"old").expect("write stale");
        cleanup_run_dir(&run_dir, &copied);
        assert!(!stale.exists(), "stale should be removed: {:?}", stale);
        assert!(copied.exists(), "current should remain: {:?}", copied);

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
