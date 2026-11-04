//! Подкоманда `light dev`: горячая перезагрузка приложения Light.
//!
//! Запускает приложение (`cargo run -p <пакет> --features dev-mode`) и
//! перезапускает его при изменении исходников, сохраняя состояние через
//! DevTools (см. [`light_fw_cli_core::dev`]).

use light_fw_cli_core::dev::{parse_dev_command, run as run_dev};

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
    /// Дополнительные флаги cargo (включая `--`, если нужно передать аргументы
    /// самому приложению). Пример: `--release`.
    #[arg(trailing_var_arg = true)]
    pub extra_cargo_args: Vec<String>,
}

/// Точка входа подкоманды `light dev`.
pub fn run(args: DevArgs) -> anyhow::Result<i32> {
    let verbose = args.verbose;
    let mut raw: Vec<String> = Vec::new();
    if let Some(package) = &args.package {
        raw.push(package.clone());
    }
    if verbose {
        raw.push("--verbose".into());
    }
    raw.extend(args.extra_cargo_args);
    // Повторно разбираем через чистую функцию `parse_dev_command`, чтобы
    // поведение CLI полностью соответствовало логике библиотеки (и чтобы
    // тесты могли покрывать именно её).
    let parsed = parse_dev_command(&raw);
    eprintln!(
        "light dev: запускаю `cargo run{} --features dev-mode {}`",
        parsed
            .package
            .as_ref()
            .map(|p| format!(" -p {p}"))
            .unwrap_or_default(),
        parsed.extra_cargo_args.join(" "),
    );
    // `run_dev` возвращает `io::Result` (ядро CLI не тянет anyhow); здесь, в
    // бинарном слое, переводим в anyhow.
    match run_dev(&parsed) {
        Ok(code) => Ok(code),
        Err(e) => Err(anyhow::anyhow!("{e}")),
    }
}
