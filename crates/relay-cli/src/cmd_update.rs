use std::path::PathBuf;

use dialoguer::Confirm;
use relay_cli_core::process::SystemRunner;
use relay_cli_core::update::{UpdateOptions, apply_update, plan_update};

/// Аргументы команды `relay update`.
#[derive(clap::Args)]
pub struct UpdateArgs {
    /// Каталог проекта (содержит `Cargo.toml`).
    #[arg(default_value = ".")]
    pub path: PathBuf,

    /// Обновить только один компонент.
    pub component: Option<String>,

    /// Не задавать вопросов.
    #[arg(long, short)]
    pub yes: bool,

    /// Показать план и не менять проект.
    #[arg(long)]
    pub dry_run: bool,

    /// Не запускать `cargo check` после обновления.
    #[arg(long)]
    pub no_verify: bool,
}

/// Обновляет компоненты проекта до актуальных версий.
pub fn run(args: UpdateArgs) -> anyhow::Result<i32> {
    let opts = UpdateOptions {
        component: args.component,
        yes: args.yes,
        dry_run: args.dry_run,
        no_verify: args.no_verify,
    };

    let plan = plan_update(&args.path, &opts)?;
    print!("{}", plan.to_text());

    if opts.dry_run {
        println!("(dry-run: изменения не применены)");
        return Ok(0);
    }

    let changed = plan.changed_count();
    if changed == 0 {
        println!("Все компоненты актуальны.");
        return Ok(0);
    }

    if !opts.yes {
        let confirmed = Confirm::new()
            .with_prompt(format!("Обновить {changed} компонент(ов)?"))
            .default(true)
            .interact()?;
        if !confirmed {
            println!("Отменено.");
            return Ok(0);
        }
    }

    apply_update(&args.path, &SystemRunner, &plan, &opts)?;
    println!("✓ обновлено компонентов: {changed}");
    Ok(0)
}
