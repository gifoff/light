use std::path::PathBuf;

use relay_cli_core::vendor::{DEFAULT_VENDOR_DIR, VendorOptions, apply_vendor, plan_vendor};

/// Аргументы команды `relay vendor`.
#[derive(clap::Args)]
pub struct VendorArgs {
    /// Каталог проекта (содержит `Cargo.toml`).
    #[arg(default_value = ".")]
    pub path: PathBuf,

    /// Каталог для исходников компонентов.
    #[arg(long, default_value = DEFAULT_VENDOR_DIR)]
    pub output: PathBuf,

    /// Показать план и не копировать.
    #[arg(long)]
    pub dry_run: bool,

    /// Режим без сети.
    #[arg(long)]
    pub offline: bool,
}

/// Копирует исходники компонентов в `vendor/`.
pub fn run(args: VendorArgs) -> anyhow::Result<i32> {
    let opts = VendorOptions {
        output: args.output,
        dry_run: args.dry_run,
        offline: args.offline,
    };

    let plan = plan_vendor(&args.path, &opts)?;
    print!("{}", plan.to_text());

    let vendored = apply_vendor(&plan, opts.dry_run)?;
    if opts.dry_run {
        println!("(dry-run: файлы не скопированы)");
    } else {
        println!("✓ вендорено компонентов: {}", vendored.len());
    }
    Ok(0)
}
