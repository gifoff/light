use std::path::PathBuf;

use relay_cli_core::vendor::{DEFAULT_VENDOR_DIR, VendorOptions, apply_vendor, plan_vendor};

/// Аргументы команды `relay vendor`.
#[derive(clap::Args)]
pub struct VendorArgs {
    /// Каталог для исходников компонентов (позиционно).
    #[arg(value_name = "PATH", conflicts_with = "path")]
    pub path_positional: Option<PathBuf>,

    /// Каталог для исходников компонентов.
    #[arg(long, value_name = "PATH")]
    pub path: Option<PathBuf>,

    /// Показать план и не копировать.
    #[arg(long)]
    pub dry_run: bool,

    /// Режим без сети.
    #[arg(long)]
    pub offline: bool,
}

/// Копирует исходники компонентов в `vendor/` текущего проекта.
pub fn run(args: VendorArgs) -> anyhow::Result<i32> {
    let output = args
        .path
        .or(args.path_positional)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_VENDOR_DIR));
    let project = std::env::current_dir()?;

    let opts = VendorOptions {
        output,
        dry_run: args.dry_run,
        offline: args.offline,
    };

    let plan = plan_vendor(&project, &opts)?;
    print!("{}", plan.to_text());

    let vendored = apply_vendor(&plan, opts.dry_run)?;
    if opts.dry_run {
        println!("(dry-run: файлы не скопированы)");
    } else {
        println!("✓ вендорено компонентов: {}", vendored.len());
    }
    Ok(0)
}
