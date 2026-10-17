use std::path::PathBuf;

use relay_cli_core::lockfile::{Lockfile, diff};

/// Аргументы команды `relay lock`.
#[derive(clap::Args)]
pub struct LockArgs {
    /// Каталог проекта (содержит `Cargo.toml`).
    #[arg(default_value = ".")]
    pub path: PathBuf,

    /// Показать изменения и не писать файл.
    #[arg(long)]
    pub dry_run: bool,
}

/// Генерирует или обновляет `relay.lock`.
pub fn run(args: LockArgs) -> anyhow::Result<i32> {
    let expected = Lockfile::from_project(&args.path)?;
    let existing = if Lockfile::path_in(&args.path).is_file() {
        Some(Lockfile::read(&args.path)?)
    } else {
        None
    };

    match &existing {
        Some(current) => {
            let changes = diff(current, &expected);
            if changes.is_empty() && current.core == expected.core {
                println!(
                    "relay.lock актуален ({} компонентов)",
                    expected.components.len()
                );
                return Ok(0);
            }
            println!("Изменения relay.lock:");
            print!("{}", changes.to_text());
            if current.core != expected.core {
                println!("  ~ relay-core: {} → {}", current.core, expected.core);
            }
        }
        None => {
            println!(
                "relay.lock отсутствует, будет создан ({} компонентов)",
                expected.components.len()
            );
        }
    }

    if args.dry_run {
        println!("(dry-run: файл не записан)");
        return Ok(0);
    }

    expected.write(&args.path)?;
    println!(
        "✓ relay.lock обновлён ({} компонентов)",
        expected.components.len()
    );
    Ok(0)
}
