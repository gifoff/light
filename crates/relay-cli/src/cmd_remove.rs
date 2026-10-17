use std::path::PathBuf;

use dialoguer::Confirm;
use relay_cli_core::process::SystemRunner;
use relay_cli_core::remove::{RemoveOptions, apply_remove, plan_remove_with_dependents};

/// Аргументы команды `relay remove`.
#[derive(clap::Args)]
pub struct RemoveArgs {
    /// Имя компонента.
    pub component: String,

    /// Каталог проекта (содержит `Cargo.toml`).
    #[arg(default_value = ".")]
    pub path: PathBuf,

    /// Не задавать вопросов.
    #[arg(long, short)]
    pub yes: bool,

    /// Показать план и не менять проект.
    #[arg(long)]
    pub dry_run: bool,

    /// Не обращаться к сети.
    #[arg(long)]
    pub offline: bool,
}

/// Удаляет компонент из проекта.
pub fn run(args: RemoveArgs) -> anyhow::Result<i32> {
    let opts = RemoveOptions {
        yes: args.yes,
        dry_run: args.dry_run,
        offline: args.offline,
    };

    let plan = plan_remove_with_dependents(&args.path, &args.component, opts.offline)?;
    print!("{}", plan.to_text());

    if opts.dry_run {
        println!("(dry-run: изменения не применены)");
        return Ok(0);
    }

    if !plan.dependents.is_empty() && !opts.yes {
        let prompt = format!(
            "От {} зависят другие компоненты. Всё равно удалить?",
            plan.name
        );
        let confirmed = Confirm::new()
            .with_prompt(prompt)
            .default(false)
            .interact()?;
        if !confirmed {
            println!("Отменено.");
            return Ok(0);
        }
    } else if plan.dependents.is_empty() && !opts.yes {
        let confirmed = Confirm::new()
            .with_prompt(format!("Удалить {}?", plan.name))
            .default(true)
            .interact()?;
        if !confirmed {
            println!("Отменено.");
            return Ok(0);
        }
    }

    apply_remove(&args.path, &SystemRunner, &plan, &opts)?;
    println!("✓ {} удалён, сборка проверена", plan.name);
    Ok(0)
}
