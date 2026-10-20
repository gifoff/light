use std::path::PathBuf;

use light_fw_cli_core::init::{InitOptions, apply};
use light_fw_cli_core::process::SystemRunner;

/// Аргументы команды `light init`.
#[derive(clap::Args)]
pub struct InitArgs {
    /// Имя нового проекта (оно же имя крейта).
    pub name: String,

    /// Каталог проекта (по умолчанию — `./<имя>`).
    #[arg(long, value_name = "PATH")]
    pub path: Option<PathBuf>,

    /// Только `Cargo.toml` и `src/main.rs` (без `.gitignore` и `light.lock`).
    #[arg(long)]
    pub minimal: bool,
}

/// Создаёт скелет нового проекта Light.
pub fn run(args: InitArgs) -> anyhow::Result<i32> {
    let options = InitOptions {
        name: args.name,
        path: args.path,
        minimal: args.minimal,
    };

    let report = apply(&SystemRunner, &options)?;
    print!("{}", report.to_text());
    Ok(0)
}
