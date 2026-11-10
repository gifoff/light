use std::path::PathBuf;

use dialoguer::Confirm;
use light_fw_cli_core::add::{AddOptions, apply_add, plan_add};
use light_fw_cli_core::process::SystemRunner;

/// Аргументы команды `light add`.
#[derive(clap::Args)]
pub struct AddArgs {
    /// Имя компонента (например, `light-fw-element-form`).
    pub component: String,

    /// Каталог проекта (содержит `Cargo.toml`).
    #[arg(default_value = ".")]
    pub path: PathBuf,

    /// Явная версия компонента.
    #[arg(long)]
    pub version: Option<String>,

    /// Не задавать вопросов.
    #[arg(long, short)]
    pub yes: bool,

    /// Показать план и не менять проект.
    #[arg(long)]
    pub dry_run: bool,

    /// Не обращаться к сети.
    #[arg(long)]
    pub offline: bool,

    /// Фича, к которой привязать зависимость (по умолчанию — имя без `light-`).
    #[arg(long)]
    pub feature: Option<String>,

    /// Фичи самого добавляемого крейта (через запятую или несколько раз).
    #[arg(long, value_delimiter = ',')]
    pub features: Vec<String>,

    /// Не запускать `cargo check` после установки.
    #[arg(long)]
    pub no_verify: bool,
}

/// Устанавливает компонент в проект.
pub fn run(args: AddArgs) -> anyhow::Result<i32> {
    let opts = AddOptions {
        version: args.version,
        yes: args.yes,
        dry_run: args.dry_run,
        offline: args.offline,
        feature: args.feature,
        features: args.features,
        no_verify: args.no_verify,
    };

    let plan = plan_add(&args.path, &args.component, &opts)?;
    print!("{}", plan.to_text());

    if plan.already_installed {
        println!(
            "Компонент {} уже установлен (версия {}).",
            plan.component.name, plan.component.version
        );
        return Ok(0);
    }

    if opts.dry_run {
        println!("(dry-run: изменения не применены)");
        return Ok(0);
    }

    if !opts.yes {
        let prompt = format!(
            "Добавить {} в фичу `{}`?",
            plan.component.name, plan.feature
        );
        let confirmed = Confirm::new()
            .with_prompt(prompt)
            .default(true)
            .interact()?;
        if !confirmed {
            println!("Отменено.");
            return Ok(0);
        }
    }

    let outcome = apply_add(&args.path, &SystemRunner, &plan, &opts)?;
    println!(
        "✓ {} добавлен (фича `{}`){}",
        outcome.component.name,
        outcome.feature,
        if outcome.verified {
            ", сборка проверена"
        } else {
            " (без проверки сборки)"
        }
    );
    for extra in &outcome.added_components {
        println!("  также добавлен {extra}");
    }
    Ok(0)
}
