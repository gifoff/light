use std::path::PathBuf;

use crate::OutputFormat;
use relay_cli_core::project::analyze_project;

/// Аргументы команды `relay check`.
#[derive(clap::Args)]
pub struct CheckArgs {
    /// Каталог проекта (содержит `Cargo.toml`).
    #[arg(default_value = ".")]
    pub path: PathBuf,

    /// Формат вывода.
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    pub format: OutputFormat,

    /// Считать проблемы неиспользуемых/отсутствующих компонентов ошибкой
    /// (код возврата 1).
    #[arg(long)]
    pub strict: bool,
}

/// Проверяет проект: сканирует шаблоны и сопоставляет их с зависимостями.
///
/// Всегда печатает отчёт. Возвращает код 1, если есть ошибки разбора
/// шаблонов; при `--strict` — также при недостающих или неиспользуемых
/// компонентах.
pub fn run(args: CheckArgs) -> anyhow::Result<i32> {
    let report = analyze_project(&args.path)?;
    match args.format {
        OutputFormat::Text => print!("{}", report.to_text()),
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&report.to_json())?),
    }

    let parse_failed = !report.scan.errors.is_empty();
    let strict_failed = args.strict && report.has_problems();
    if parse_failed || strict_failed {
        return Ok(1);
    }
    Ok(0)
}
