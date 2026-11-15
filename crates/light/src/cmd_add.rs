use std::io::{IsTerminal, Write};
use std::path::PathBuf;

use dialoguer::Confirm;
use light_fw_cli_core::add::{AddOptions, apply_add, plan_add};
use light_fw_cli_core::error::Error;
use light_fw_cli_core::process::SystemRunner;

/// Аргументы команды `light add`.
#[derive(Clone, clap::Args)]
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

    /// Разрешить «вытеснение» уже установленного компонента при конфликте по
    /// тегам элемента (`provides.elements`). Приоритет фиксируется в секции
    /// `[plugins]` файла `light.lock`.
    #[arg(long)]
    pub force: bool,
}

/// Устанавливает компонент в проект.
///
/// Этап 2 Plugin API: при конфликте по тегам элементной разметки CLI
/// предлагает интерактивно выбрать (1/2) или сразу `--force`/`--yes`.
/// Повторный вызов [`try_add`] с `force=true` — наш путь.
pub fn run(args: AddArgs) -> anyhow::Result<i32> {
    let err = match try_add(&args) {
        Ok(code) => return Ok(code),
        Err(e) => e,
    };
    if let Some(Error::Conflict {
        tag,
        existing_crate,
        new_crate,
    }) = err.downcast_ref::<Error>()
    {
        resolve_conflict_interactively(&args, tag, existing_crate, new_crate)
    } else {
        Err(err)
    }
}

/// Основной поток работы `light add`. Отдельно от [`run`] для повторяемости
/// при повторе с `--force` (см. [`resolve_conflict_interactively`]).
fn try_add(args: &AddArgs) -> anyhow::Result<i32> {
    let opts = AddOptions {
        version: args.version.clone(),
        yes: args.yes,
        dry_run: args.dry_run,
        offline: args.offline,
        feature: args.feature.clone(),
        features: args.features.clone(),
        no_verify: args.no_verify,
        force: args.force,
        // ТЗ 0.17.2: ручной `light add` никогда не отключает дефолтные фичи
        // `light-fw`; прописываем явно, чтобы опция была видимой в месте вызова.
        default_features: true,
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

    if !opts.yes && !opts.force {
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
    // Этап 2 Plugin API: сообщаем о применении приоритетов из [plugins].
    for (tag, winner) in &plan.plugin_priorities {
        println!("  приоритет: {tag} → {winner} (зафиксировано в light.lock)");
    }
    Ok(0)
}

/// Этап 2 Plugin API: обработка [`Error::Conflict`] при конфликте по тегам.
///
/// - **TTY**: интерактивный выбор 1/2 — «Оставить» / «Использовать новый».
///   «Оставить» → exit 0 без изменений; «Использовать новый» → повтор с
///   `force: true` (приоритет пишется в `[plugins]` `light.lock`).
/// - **Не-TTY** (CI, пайп, скрипты): ошибка с инструкцией (добавить `--force`
///   или запустить интерактивно) → exit 1.
///
/// Если `existing_crate == new_crate` (конфликт по версии повторного `add`) —
/// возвращаем ошибку с тем же текстом из `tag` (это не элементный конфликт).
fn resolve_conflict_interactively(
    args: &AddArgs,
    tag: &str,
    existing_crate: &str,
    new_crate: &str,
) -> anyhow::Result<i32> {
    // Повторный `add` с неверной версией: диагностика уже в `tag`.
    if existing_crate == new_crate {
        eprintln!("⚠️ {tag}");
        eprintln!(
            "   Запустите `light add {component} --version <версия>` с текущей \
             версией или без `--version`.",
            component = args.component
        );
        return Err(anyhow::anyhow!("{tag}"));
    }
    eprintln!("⚠️ Конфликт: тег `{tag}` уже предоставлен компонентом `{existing_crate}`.");
    eprintln!("   Новый компонент `{new_crate}` тоже предоставляет `{tag}`.");
    // Быстрый выход: уже указали --force или --yes — повтор сразу с force.
    if args.force || args.yes {
        eprintln!("   Продолжаю с --force (приоритет будет записан в light.lock).");
        let mut retry = args.clone();
        retry.force = true;
        return try_add(&retry);
    }
    if !std::io::stdin().is_terminal() {
        eprintln!(
            "   В неинтерактивном режиме добавьте `--force`, если хотите \
             использовать `{new_crate}` вместо `{existing_crate}`."
        );
        return Err(anyhow::anyhow!(
            "конфликт по тегу `{tag}`: выберите --force (использовать новый) \
             или запустите интерактивно"
        ));
    }
    eprintln!("   Какой использовать? [1/2]:");
    eprintln!("     1. Оставить текущий (отменить установку)");
    eprintln!("     2. Использовать новый ({new_crate}) -- записать приоритет в light.lock");
    loop {
        print!("   Ваш выбор: ");
        std::io::stdout().flush()?;
        let mut line = String::new();
        std::io::stdin().read_line(&mut line)?;
        match line.trim() {
            "1" => {
                println!("Отменено. Проект не изменён.");
                return Ok(0);
            }
            "2" => {
                let mut retry = args.clone();
                retry.force = true;
                return try_add(&retry);
            }
            other => eprintln!("   Неверный ввод `{other}`, попробуйте снова (1 или 2)."),
        }
    }
}
