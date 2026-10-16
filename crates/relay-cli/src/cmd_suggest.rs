use std::path::PathBuf;

use crate::OutputFormat;
use relay_cli_core::project::analyze_project;

/// Аргументы команды `relay suggest`.
#[derive(clap::Args)]
pub struct SuggestArgs {
    /// Каталог проекта (содержит `Cargo.toml`).
    #[arg(default_value = ".")]
    pub path: PathBuf,

    /// Формат вывода.
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    pub format: OutputFormat,
}

/// Подсказывает, какие компоненты нужно добавить, и всегда возвращает код 0.
pub fn run(args: SuggestArgs) -> anyhow::Result<i32> {
    let report = analyze_project(&args.path)?;
    match args.format {
        OutputFormat::Text => {
            if report.recommendations.is_empty() {
                println!("Все используемые компоненты уже подключены.");
            } else {
                for r in &report.recommendations {
                    println!("добавить {}", r.requirement);
                }
            }
        }
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&report.to_json())?),
    }
    Ok(0)
}
