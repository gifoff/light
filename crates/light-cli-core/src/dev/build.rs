use std::path::{Path, PathBuf};
use std::process::Child;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::args::{RELOAD_POLL_INTERVAL, RELOAD_WAIT_MAX};
use super::watch::{is_port_open, request_reload_template};

pub(super) const FILE_HASH_RETRY_INTERVAL: Duration = Duration::from_millis(50);
pub(super) const FILE_HASH_MAX_ATTEMPTS: u32 = 3;

/// Вычисляет хеш файла (SHA-256) для сравнения при hot-reload.
pub fn file_hash(path: &Path, verbose: bool) -> Option<u64> {
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
                if verbose {
                    eprintln!(
                        "light dev: hash({path:?}) read attempt {attempts} failed: {e} \
                         (повторю через {:?}, всего до {FILE_HASH_MAX_ATTEMPTS:?} попыток)",
                        FILE_HASH_RETRY_INTERVAL
                    );
                }
                thread::sleep(FILE_HASH_RETRY_INTERVAL);
            }
        }
    };
    let result: Option<u64> = buf.map(|b| {
        let mut hasher = DefaultHasher::new();
        b.hash(&mut hasher);
        hasher.finish()
    });
    if verbose {
        eprintln!("light dev: hash({:?}) = {:?}", path, result);
    }
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
    #[cfg_attr(not(windows), allow(unused_mut))]
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
    #[cfg_attr(not(windows), allow(unused_mut))]
    let mut p = run_dir.join(format!("{bin}-{ts}"));
    #[cfg(windows)]
    {
        p.set_extension("exe");
    }
    p
}

/// Удаляет все старые копии бинарника в `run_dir`, кроме активного (`keep`).
/// На Windows — файлы с расширением `.exe`; на Unix — файлы без расширения
/// (бесшовные бинарники). Ошибки молча игнорируются: файл мог быть ещё жив
/// в AV-кэше.
pub fn cleanup_run_dir(run_dir: &Path, keep: &Path) {
    if let Ok(entries) = std::fs::read_dir(run_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path == keep {
                continue;
            }
            #[cfg(windows)]
            {
                // На Windows копии всегда заканчиваются `.exe`.
                if path.extension().and_then(|e| e.to_str()) == Some("exe") {
                    let _ = std::fs::remove_file(&path);
                }
            }
            #[cfg(not(windows))]
            {
                // На Unix бинарник копируется без расширения (`<bin>-<ts>`),
                // поэтому чистим файлы без расширения, чтобы не удалить
                // случайный файл с `.exe`-суффиксом (не наш формат).
                if path.extension().is_none() {
                    let _ = std::fs::remove_file(&path);
                }
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
