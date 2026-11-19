use std::time::Duration;

/// Интервал опроса файловой системы [`PollWatcher`]. На UNC- и сетевых дисках
/// нативный watcher события не ловит, поэтому используем периодическое
/// опросование (500 мс — разумный баланс между задержкой реакции и нагрузкой).
pub(super) const DEVTOOLS_PORT: u16 = 9229;
pub(super) const POLL_INTERVAL: Duration = Duration::from_millis(500);
/// Максимум времени, которое наблюдатель игнорирует события, пока приложение
/// собирается/стартует. Защита от вечной блокировки: если порт 9229 так и не
/// открылся (например, сборка зависла), по истечении этого дедлайна наблюдение
/// всё равно возобновляется.
pub(super) const STARTUP_IGNORE: Duration = Duration::from_secs(30);
/// Максимум времени ожидания восстановления TCP-доступа к порту 9229 после
/// неудачной отправки `RELOAD_TEMPLATE` (приложение могло умереть или
/// пересобраться). Каждые 200 мс пробуем TCP-подключение; если порт снова
/// принимает соединения — повторяем `RELOAD_TEMPLATE` (без полной пересборки).
pub(super) const RELOAD_WAIT_MAX: Duration = Duration::from_secs(3);
/// Период опроса TCP при ожидании восстановления после неудачного reload.
pub(super) const RELOAD_POLL_INTERVAL: Duration = Duration::from_millis(200);
/// Дебаунс для изменений `.rs`/`Cargo.toml` (полная пересборка + рестарт).
pub(super) const RS_DEBOUNCE: Duration = Duration::from_millis(500);
/// Дебаунс для изменений `.html` (runtime-перезагрузка шаблона без пересборки).
pub(super) const HTML_DEBOUNCE: Duration = Duration::from_millis(100);

/// Разобранные параметры команды `light dev`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DevArgs {
    /// Пакет воркспейса, который запускается (копия из `target/hot-run/`).
    pub package: Option<String>,
    /// Дополнительные флаги cargo, передаваемые после имени пакета
    /// (например, `--release`).
    pub extra_cargo_args: Vec<String>,
    /// Включить подробные логи (debounce, hash-dedup, notify events).
    pub verbose: bool,
    /// Полный сброс кэша dev-сборки перед стартом (hot-build, hot-run, state,
    /// deps текущего пакета). Смысл — при смене версии фреймворка, когда
    /// auto-invalidation уже не помогает.
    pub clean: bool,
    /// Сохранять state.json между ВСЕМИ запусками (в т.ч. cold start).
    /// Эквивалент LIGHT_PRESERVE_STATE=1. По умолчанию false.
    pub preserve_state: bool,
    /// ТЗ 0.16.0: запустить в режиме агента — автоматически создавать
    /// session recorder, логировать все DevTools-команды и генерировать
    /// summary.md при завершении процесса.
    pub agent: bool,
    /// ТЗ 0.18.0: не запускать web UI DevTools (по умолчанию включён).
    pub no_web: bool,
    /// ТЗ 0.18.0: порт web UI DevTools (по умолчанию 9228; при указании
    /// web-сервер включается даже если в конфиге `enabled: false`).
    pub web_port: Option<u16>,
}

/// Чистый разбор сырых аргументов командной строки в [`DevArgs`].
///
/// Правила: первый аргумент (если он существует и не начинается с `-`)
/// считается именем пакета; `--verbose` включает подробные логи; всё
/// остальное попадает в `extra_cargo_args`. Это изолировано от `clap`,
/// чтобы тестировать парсинг без запуска бинарника.
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
/// assert!(!args.verbose);
///
/// // `light dev --verbose`
/// let args = parse_dev_command(&["--verbose".into()]);
/// assert!(args.verbose);
/// assert_eq!(args.package, None);
/// assert!(args.extra_cargo_args.is_empty());
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
        if arg == "--verbose" {
            out.verbose = true;
        } else if arg == "--clean" {
            // Полный сброс кэша (TZ 0.14.4): не передаём в cargo, а обрабатываем
            // до warm-up build.
            out.clean = true;
        } else if arg == "--preserve-state" {
            // ТЗ 0.14.3-fix4: сохранять state между всеми запусками.
            out.preserve_state = true;
        } else if arg == "--agent" {
            // ТЗ 0.16.0: режим агента (session recorder + summary.md).
            out.agent = true;
        } else if arg == "--no-web" {
            // ТЗ 0.18.0: не запускать web UI DevTools.
            out.no_web = true;
        } else if let Some(port_str) = arg.strip_prefix("--web-port=") {
            // ТЗ 0.18.0: порт web UI DevTools.
            if let Ok(p) = port_str.parse::<u16>() {
                out.web_port = Some(p);
            }
        } else if i == 0 && !arg.starts_with('-') {
            out.package = Some(arg.clone());
        } else {
            out.extra_cargo_args.push(arg.clone());
        }
    }
    out
}
