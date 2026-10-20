use std::path::PathBuf;

use light_fw_cli_core::init::{InitComponent, InitOptions, apply};
use light_fw_cli_core::process::SystemRunner;

fn parse_component(value: &str) -> Result<InitComponent, String> {
    InitComponent::parse(value).map_err(|error| error.to_string())
}

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

    /// Подключить компонент (`form`, `validation`). Можно указывать несколько раз.
    #[arg(long = "with", value_name = "COMPONENT", value_parser = parse_component)]
    pub with: Vec<InitComponent>,
}

/// Создаёт скелет нового проекта Light.
pub fn run(args: InitArgs) -> anyhow::Result<i32> {
    let options = InitOptions {
        name: args.name,
        path: args.path,
        minimal: args.minimal,
        components: args.with,
    };

    let report = apply(&SystemRunner, &options)?;
    print!("{}", report.to_text());
    Ok(0)
}
