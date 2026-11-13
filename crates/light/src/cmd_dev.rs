//! Подкоманда `light dev`: горячая перезагрузка приложения Light.
//!
//! Запускает приложение (`cargo run -p <пакет> --features dev-mode`) и
//! перезапускает его при изменении исходников, сохраняя состояние через
//! DevTools (см. [`light_fw_cli_core::dev`]).

use std::path::Path;

use light_fw_cli_core::check::ensure_components_present;
use light_fw_cli_core::dev::{parse_dev_command, run as run_dev};
use light_fw_devtools_web::{config, server};

/// Аргументы подкоманды `dev`.
///
/// Первый свободный аргумент (не начинающийся с `-`) — имя пакета; `--verbose`
/// включает подробные логи; всё остальное передаётся в `cargo run` как есть
/// (например, `--release`).
#[derive(clap::Args, Debug)]
pub struct DevArgs {
    /// Пакет воркспейса, который запускается.
    ///
    /// Если не указан, `cargo run` запустит пакет по умолчанию (из текущего
    /// каталога).
    pub package: Option<String>,
    /// Включить подробные логи (debounce, hash-dedup, notify events).
    #[arg(long, default_value_t = false)]
    pub verbose: bool,
    /// Полный сброс кэша dev-сборки перед стартом (hot-build, hot-run, state
    /// и артефакты deps текущего пакета). Смысл — при смене версии фреймворка,
    /// когда auto-invalidation уже не помогает (например, переезд между ветками
    /// или ручная замена исходников light-fw-core).
    #[arg(long, default_value_t = false)]
    pub clean: bool,
    /// Сохранять state.json между ВСЕМИ запусками (в т.ч. cold start).
    /// Эквивалент LIGHT_PRESERVE_STATE=1. По умолчанию false.
    #[arg(long, default_value_t = false)]
    pub preserve_state: bool,
    /// ТЗ 0.16.0: режим агента — автоматически создавать session recorder,
    /// логировать все DevTools-команды и генерировать summary.md при выходе.
    #[arg(long, default_value_t = false)]
    pub agent: bool,
    /// Не устанавливать недостающие компоненты автоматически (только показать
    /// список и инструкции `light add`).
    #[arg(long, default_value_t = false)]
    pub no_auto_install: bool,
    /// ТЗ 0.18.0: не запускать web UI DevTools (по умолчанию включён).
    #[arg(long, default_value_t = false)]
    pub no_web: bool,
    /// ТЗ 0.18.0: порт web UI DevTools (по умолчанию 9228; при указании
    /// web-сервер включается даже если в конфиге `enabled: false`).
    #[arg(long)]
    pub web_port: Option<u16>,
    /// Дополнительные флаги cargo (включая `--`, если нужно передать аргументы
    /// самому приложению). Пример: `--release`.
    #[arg(trailing_var_arg = true)]
    pub extra_cargo_args: Vec<String>,
}

/// Точка входа подкоманды `light dev`.
pub fn run(args: DevArgs) -> anyhow::Result<i32> {
    // Автоматическая проверка компонентов перед запуском.
    let project_root = Path::new(".").to_path_buf();
    if project_root.join("Cargo.toml").exists() {
        let install_fn = |name: &str| -> light_fw_cli_core::Result<()> {
            // ТЗ 0.17.2: автоподгрузка явно уважает дефолтные фичи `light-fw`
            // (не отключает их); опция прописана в обоих вызовах.
            let opts = light_fw_cli_core::add::AddOptions {
                default_features: true,
                ..Default::default()
            };
            let plan = light_fw_cli_core::add::plan_add(&project_root, name, &opts)?;
            let _ = light_fw_cli_core::add::apply_add(
                &project_root,
                &light_fw_cli_core::process::SystemRunner,
                &plan,
                &opts,
            )?;
            Ok(())
        };
        let auto_install = !args.no_auto_install;
        let ready = ensure_components_present(&project_root, &install_fn, auto_install)?;
        if !ready {
            return Ok(1);
        }
    }

    let verbose = args.verbose;
    let mut raw: Vec<String> = Vec::new();
    if let Some(package) = &args.package {
        raw.push(package.clone());
    }
    if verbose {
        raw.push("--verbose".into());
    }
    if args.clean {
        raw.push("--clean".into());
    }
    if args.preserve_state {
        raw.push("--preserve-state".into());
    }
    if args.agent {
        raw.push("--agent".into());
    }
    if args.no_web {
        raw.push("--no-web".into());
    }
    if let Some(port) = args.web_port {
        raw.push(format!("--web-port={port}"));
    }
    raw.extend(args.extra_cargo_args);
    // Повторно разбираем через чистую функцию `parse_dev_command`, чтобы
    // поведение CLI полностью соответствовало логике библиотеки (и чтобы
    // тесты могли покрывать именно её).
    let parsed = parse_dev_command(&raw);
    eprintln!(
        "⏳ Запуск (cargo run{} --features dev-mode {})",
        parsed
            .package
            .as_ref()
            .map(|p| format!(" -p {p}"))
            .unwrap_or_default(),
        parsed.extra_cargo_args.join(" "),
    );
    // ТЗ 0.18.0: web UI DevTools — запускаем ДО `run_dev()`, чтобы сервер был
    // готов, когда приложение откроет TCP 9229. Приоритет: --no-web >
    // --web-port (implies enabled) > config `dev.devtools.web.enabled:false`
    // > default (enabled, port 9228).
    let web = if !args.no_web {
        let cfg_json = std::fs::read_to_string("light.config.json").unwrap_or_default();
        let web_cfg = config::parse(&cfg_json);
        let effective_enabled = args.web_port.is_some() || web_cfg.enabled;
        if effective_enabled {
            let opts = server::ServeOptions {
                config: light_fw_devtools_web::config::WebConfig {
                    enabled: true,
                    port: args.web_port.unwrap_or(web_cfg.port),
                },
                devtools_port: 9229,
            };
            Some(server::serve(opts, args.web_port))
        } else {
            eprintln!("light dev: web UI DevTools disabled (config)");
            None
        }
    } else {
        None
    };

    // `run_dev` возвращает `io::Result` (ядро CLI не тянет anyhow); здесь, в
    // бинарном слое, переводим в anyhow.
    let code = match run_dev(&parsed) {
        Ok(code) => code,
        Err(e) => return Err(anyhow::anyhow!("{e}")),
    };

    // ТЗ 0.18.0: останавливаем web UI DevTools (блокирующее join до 3 c).
    if let Some(w) = web {
        w.stop();
    }

    Ok(code)
}
