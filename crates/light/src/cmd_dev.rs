//! Подкоманда `light dev`: горячая перезагрузка приложения Light.
//!
//! Запускает приложение (`cargo run -p <пакет> --features dev-mode`) и
//! перезапускает его при изменении исходников, сохраняя состояние через
//! DevTools (см. [`light_fw_cli_core::dev`]).

use std::io::IsTerminal;
use std::path::Path;

use light_fw_cli_core::check::{build_check_report, ensure_components_present, parse_yn};
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
    /// ТЗ 0.21.0: фоновый запуск — CLI спавнит полный процесс `light dev`
    /// (супервизор + приложение + web UI) через `std::process::Command`,
    /// печатает его PID и порты DevTools в stdout и сразу возвращает управление
    /// терминалу (exit 0). AI-агентам больше не нужен PowerShell/Start-Process.
    /// Остановка: `light devtools session stop` (через TCP 9229) или `taskkill
    /// /PID <pid>` на Windows.
    #[arg(long, default_value_t = false)]
    pub detach: bool,
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
            // `no_verify: true` — автоустановка из `light dev` **не** запускает
            // отдельный `cargo check` (регрессия 0.20.2: бесконечный `cargo check`
            // без таймаута «замораживал» `light dev` в non-TTY/CI). Корректность
            // добавленной зависимости тут же проверяет сама последующая сборка
            // приложения — `run_dev` сразу делает `cargo run --features dev-mode`.
            let opts = light_fw_cli_core::add::AddOptions {
                default_features: true,
                no_verify: true,
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

        // Этап 6 Plugin API: авто-детект кастомных тегов (`<datepicker />` и
        // прочие «неизвестные стандартному HTML» теги из шаблонов проекта).
        // `ensure_components_present` выше смотрит ТОЛЬКО standard-component
        // зависимости; здесь — теги, которые должен предоставлять плагин.
        //
        //   * `available` (тег в bundled/remote реестре, плагин не установлен)
        //     → TTY: спросить «Установить <crate>? [Y/n]»; Y или non-TTY →
        //     `plan_add`/`apply_add`; N → warn и продолжить (сборка может
        //     упасть, это на совести пользователя);
        //   * `unknown` (тега нет ни в ядре, ни в реестре) → error +
        //     подсказка `light search <tag>` и выход с кодом 1.
        //
        let check_report = match build_check_report(&project_root) {
            Ok(report) => report,
            Err(e) => {
                eprintln!("light dev: не удалось построить отчёт `light check`: {e}");
                return Ok(1);
            }
        };
        if !check_report.available.is_empty() || !check_report.unknown.is_empty() {
            eprintln!("\n🔍 Light: обнаружены кастомные теги в шаблонах проекта:\n");
            for a in &check_report.available {
                eprintln!(
                    "   🟡 <{}> — доступно (не установлено): {}",
                    a.tag, a.requirement
                );
            }
            for t in &check_report.unknown {
                eprintln!(
                    "   🔴 <{t}> — неизвестно (нет ни в ядре (light-fw-core), ни в реестре Light"
                );
            }
        }
        for a in &check_report.available {
            let mut install = true;
            if !auto_install {
                // `--no-auto-install`: только показать список и инструкции
                // `light add`; ничего не устанавливаем.
                install = false;
                eprintln!(
                    "   → вручную: light add {} (--feature / --features при необходимости)",
                    a.crate_name
                );
            } else if !args.agent && std::io::stdin().is_terminal() {
                // TTY-режим (без `--agent`): спрашиваем пользователя (Y по
                // умолчанию). В режиме агента (`--agent`) вопрос не задаётся —
                // устанавливаем автоматически, чтобы не блокировать pipeline.
                print!(
                    "   Установить {}? [Y/n] ",
                    a.requirement
                        .split_whitespace()
                        .next()
                        .unwrap_or(a.crate_name.as_str())
                );
                std::io::Write::flush(&mut std::io::stderr()).ok();
                let mut line = String::new();
                if std::io::stdin().read_line(&mut line).is_err() {
                    // В CI/пайплайне stdin может закрыться — считаем «да»
                    // (auto-install по умолчанию включён).
                    eprintln!("(ответ не получен, устанавливаю автоматически)");
                } else if !parse_yn(&line) {
                    install = false;
                    eprintln!(
                        "   ⚠️ Отказано. Сборка может не пройти, пока <{}> не подключён.",
                        a.tag
                    );
                }
            }
            // non-TTY + auto_install → install остаётся `true` (автоустановка).
            if install {
                eprintln!("⏳ Устанавливаю {} …", a.crate_name);
                if let Err(e) = (install_fn)(&a.crate_name) {
                    eprintln!(
                        "❌ Не удалось установить {}: {e}\n   Попробуйте вручную: light add {}",
                        a.crate_name, a.crate_name
                    );
                    return Ok(1);
                }
                eprintln!("✅ {} установлен", a.crate_name);
            }
        }
        if !check_report.unknown.is_empty() {
            eprintln!();
            for t in &check_report.unknown {
                eprintln!(
                    "   ❌ <{t}> — неизвестный тег. Проверьте опечатку или найдите плагин: `light search {t}`"
                );
            }
            eprintln!();
            return Ok(1);
        }
    }

    // TЗ 0.21.0: `--detach` — фоновый запуск: респавним **себя** с теми же
    // аргументами (кроме самого `--detach`) как отдельный процесс, накрываем
    // stdout/stderr файлами-лечами (чтобы дочерний не перехватывал TTY),
    // печатаем PID и порты в stdout и возвращаем управление терминалу (exit 0).
    // Так агенту не нужен PowerShell/Start-Process/Win32 API, и весь
    // функционал (hot-reload, web UI, agent session, hot-restart) работает
    // без изменений — это ровно тот же бинарник, только спавнутый.
    // Важно: проверяем ДО перемещения полей `args` (иначе partial-move error).
    if args.detach {
        return run_detached(&args, &project_root);
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

/// ТЗ 0.21.0: фоновый запуск `light dev --detach`.
///
/// Спавнит **себя** (те же аргументы, но без `--detach`) как отдельный процесс
/// с stdout/stderr, перенаправленными в файл-лог `.light/agent-sessions/dev-detach.log`
/// (создаётся каталог при необходимости — он уже используется для
/// `--agent`). Родитель сразу печатает JSON-ответ с PID и портами и выходит
/// с кодом 0, не дожидаясь дочернего.
///
/// Выход через `std::process::exit(0)` намеренный: это terminal-операция
/// (агент получает управление терминалом обратно); `anyhow::Result` не нужен,
/// ошибки спавна обрабатываются отдельно.
fn run_detached(args: &DevArgs, project_root: &Path) -> anyhow::Result<i32> {
    use std::fs::OpenOptions;
    use std::process::{Command, Stdio};

    // Путь к текущему бинарнику (родительский процесс `light`).
    let self_path = std::env::current_exe()?;

    // Файл-лежачек для stdout+stderr дочернего: `.light/agent-sessions/dev-detach.log`.
    // Каталог `.light/agent-sessions/` уже создаётся при `--agent`; при отсутствии
    // `--agent` мы его создаём сами. Имя файла фиксированное (один dev-detach.log),
    // т.к. одновременно может быть только один живой `light dev --detach` на порт 9229.
    let sessions_dir = project_root.join(".light").join("agent-sessions");
    std::fs::create_dir_all(&sessions_dir)?;
    let log_path = sessions_dir.join("dev-detach.log");
    // Дочерний пишет и stdout, и stderr в один и тот же файл (append-режим):
    // открываем два отдельных ручья (file handles) на один путь — stdio не
    // позволяет использовать один `File` для двух потоков одновременно.
    let stdout_handle = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;
    let stderr_handle = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;

    // Аргументы дочернего: `dev` + все те же аргументы, кроме `--detach`.
    // `--detach` исключён из argv (иначе рекурсивный detach).
    let mut child_args: Vec<String> = vec!["dev".into()];
    if let Some(p) = &args.package {
        child_args.push(p.clone());
    }
    if args.verbose {
        child_args.push("--verbose".into());
    }
    if args.clean {
        child_args.push("--clean".into());
    }
    if args.preserve_state {
        child_args.push("--preserve-state".into());
    }
    if args.agent {
        child_args.push("--agent".into());
    }
    if args.no_auto_install {
        child_args.push("--no-auto-install".into());
    }
    if args.no_web {
        child_args.push("--no-web".into());
    }
    if let Some(port) = args.web_port {
        child_args.push(format!("--web-port={port}"));
    }
    child_args.extend(args.extra_cargo_args.clone());

    eprintln!(
        "light dev: --detach — запускаю в фоне: {:?} (лог → {})",
        child_args,
        log_path.display()
    );

    // Запускаем. `kill_on_drop` не нужен: дочерний переживёт родителя
    // (не привязан к его жизненному циклу), на Windows и Unix.
    let mut cmd = Command::new(&self_path);
    cmd.args(&child_args);
    cmd.stdout(Stdio::from(stdout_handle));
    cmd.stderr(Stdio::from(stderr_handle));
    // Не наследуем stdin (чтобы дочерний не блокировался на prompt):
    cmd.stdin(Stdio::null());
    // Явно указываем cwd (родитель мог быть запущен из другого места).
    cmd.current_dir(project_root);

    // Windows: дочерний `light dev` не должен открывать консольное окно —
    // иначе `--detach` в терминале агента (или в CI) поднимает второе окно
    // наверх и засоряет вывод. `CREATE_NO_WINDOW` (0x08000000) запрещает
    // создание console window у дочернего; stdout/stderr уже перенаправлены
    // в лог, так что ничего не теряется.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let child = cmd.spawn()?;
    let pid = child.id();

    // Не ждём дочернего: агент должен получить управление терминалом сразу.
    // Дочерний переживёт родителя (не привязан к его жизненному циклу).
    drop(child);

    // Вывод для агента: JSON с PID, портом DevTools, портом web UI, путём к логу.
    // Это stdout (по ТЗ), stderr остаётся пустым (мы уже ничего там не печатали
    // после eprintln выше — тот ушёл в TTY родителя).
    println!(
        "{{\"ok\":true,\"pid\":{},\"devtools_port\":9229,\"web_port\":{},\"log\":\"{}\"}}",
        pid,
        args.web_port.unwrap_or(9228),
        log_path.display()
    );
    println!("PID: {pid}");
    println!("DevTools port: 9229");
    println!(
        "Web UI: http://127.0.0.1:{}/",
        args.web_port.unwrap_or(9228)
    );
    println!("Log: {}", log_path.display());
    println!("Остановка: light devtools session stop   (или taskkill /PID {pid} на Windows)");
    // Короткая пауза (500 мс): дочерний успевает создать сессию/сервер и
    // занять порты; иначе первая команда агента (`light devtools shot`) может
    // не застать сервер. Терминал возвращается сразу — это не блокирует.
    std::thread::sleep(std::time::Duration::from_millis(500));

    std::process::exit(0);
}
