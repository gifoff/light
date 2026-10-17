use std::path::PathBuf;

use crate::OutputFormat;
use relay_cli_core::doctor::{SystemProbe, doctor};

/// Аргументы команды `relay doctor`.
#[derive(clap::Args)]
pub struct DoctorArgs {
    /// Каталог проекта (содержит `Cargo.toml`).
    #[arg(default_value = ".")]
    pub path: PathBuf,

    /// Формат вывода.
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    pub format: OutputFormat,

    /// Не проверять доступность сети.
    #[arg(long)]
    pub offline: bool,
}

/// Диагностирует окружение и проект. Возвращает код 1 при ошибках.
pub fn run(args: DoctorArgs) -> anyhow::Result<i32> {
    let probe = if args.offline {
        SystemProbe::offline()
    } else {
        SystemProbe::new()
    };
    let report = doctor(&args.path, &probe)?;
    match args.format {
        OutputFormat::Text => print!("{}", report.to_text()),
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&report.to_json())?),
    }
    Ok(if report.has_errors() { 1 } else { 0 })
}
