use std::collections::BTreeSet;
use std::io::IsTerminal;

use anyhow::anyhow;
use dialoguer::Confirm;
use relay_cli_core::process::SystemRunner;
use relay_cli_core::publish::{
    self, CratesIoProbe, PublishInvocation, RetryConfig, SystemSleeper, Workspace,
};

/// Аргументы команды `relay publish`.
#[derive(clap::Args)]
pub struct PublishArgs {
    /// Имя крейта для публикации (по умолчанию — весь воркспейс).
    #[arg(value_name = "CRATE", conflicts_with = "only")]
    pub crate_name: Option<String>,

    /// Имя крейта для публикации (эквивалент позиционного аргумента).
    #[arg(long, value_name = "CRATE")]
    pub only: Option<String>,

    /// Проверить упаковку, не публикуя (используется `cargo package --list`).
    #[arg(long)]
    pub dry_run: bool,

    /// Не задавать вопросов.
    #[arg(long, short)]
    pub yes: bool,

    /// Разрешить грязное рабочее дерево.
    #[arg(long)]
    pub allow_dirty: bool,

    /// Токен реестра (по умолчанию — `cargo login`/`CARGO_REGISTRY_TOKEN`).
    #[arg(long, value_name = "TOKEN")]
    pub token: Option<String>,

    /// Не запускать `cargo verify` при публикации (по умолчанию).
    #[arg(long)]
    pub no_verify: bool,

    /// Выполнить полную верификацию `cargo publish`.
    #[arg(long, conflicts_with = "no_verify")]
    pub verify: bool,

    /// Пропустить pre-flight гейты (`build`/`test`/`clippy`/`fmt`).
    #[arg(long)]
    pub skip_gates: bool,
}

/// Публикует крейты воркспейса в crates.io в топологическом порядке.
pub fn run(args: PublishArgs) -> anyhow::Result<i32> {
    let cwd = std::env::current_dir()?;
    let runner = SystemRunner;
    let metadata = publish::load_metadata(&runner, &cwd)?;
    let exclude = publish::running_package_exclude(&metadata);
    let workspace = Workspace::from_metadata(metadata)?;
    let only = args.only.or(args.crate_name);
    let plan = publish::plan(&workspace, only.as_deref())?;

    if !plan.skipped.is_empty() {
        println!(
            "skipped {} crates (publish = false): {}",
            plan.skipped.len(),
            plan.skipped.join(", ")
        );
    }
    for warning in publish::metadata_warnings(&workspace, &plan) {
        eprintln!("warning: {warning}");
    }

    if plan.order.is_empty() {
        println!("Нечего публиковать.");
        return Ok(0);
    }

    if !args.dry_run && !args.yes {
        if !std::io::stdin().is_terminal() {
            return Err(anyhow!(
                "refusing to publish in non-interactive mode without --yes"
            ));
        }
        let list = plan
            .order
            .iter()
            .map(|name| {
                let version = workspace
                    .package(name)
                    .map(|package| package.version.to_string())
                    .unwrap_or_else(|| "?".to_string());
                format!("{name} {version}")
            })
            .collect::<Vec<_>>()
            .join(", ");
        let confirmed = Confirm::new()
            .with_prompt(format!(
                "Publish {} crates to crates.io: {list}. Continue?",
                plan.order.len()
            ))
            .default(false)
            .interact()?;
        if !confirmed {
            println!("Отменено.");
            return Ok(0);
        }
    }

    let total_steps = if args.dry_run { 2 } else { 3 };

    let mut preflight = Vec::new();
    if args.skip_gates {
        preflight.push("gates skipped".to_string());
    } else {
        let gates = publish::run_gates(&runner, &workspace.root, exclude.as_deref())?;
        preflight.extend(
            gates
                .iter()
                .map(|gate| format!("{} {}", gate.name, if gate.ok { "✓" } else { "✗" })),
        );
        if let Some(name) = &exclude {
            preflight.push(format!("excluded {name} (running binary)"));
        }
        if let Some(failed) = gates.iter().find(|gate| !gate.ok) {
            println!("[1/{total_steps}] pre-flight: {}", preflight.join(" "));
            eprintln!(
                "pre-flight failed: cargo {}:\n{}",
                failed.name, failed.stderr
            );
            return Ok(1);
        }
    }

    if let Some(message) = publish::check_demo(&workspace)? {
        preflight.push("relay check ✗".to_string());
        println!("[1/{total_steps}] pre-flight: {}", preflight.join(" "));
        eprintln!("{message}");
        return Ok(1);
    }
    preflight.push("relay check ✓".to_string());

    if let Err(error) = publish::validate_manifests(&workspace, &plan) {
        preflight.push("metadata ✗".to_string());
        println!("[1/{total_steps}] pre-flight: {}", preflight.join(" "));
        eprintln!("{error}");
        return Ok(1);
    }
    preflight.push("metadata ✓".to_string());

    if let Err(error) = publish::check_version_consistency(&workspace, &plan) {
        preflight.push("versions ✗".to_string());
        println!("[1/{total_steps}] pre-flight: {}", preflight.join(" "));
        eprintln!("{error}");
        return Ok(1);
    }
    println!("[1/{total_steps}] pre-flight: {}", preflight.join(" "));

    let invocation = PublishInvocation {
        verify: args.verify,
        allow_dirty: args.allow_dirty,
        token: args.token.clone(),
    };

    if args.dry_run {
        println!("[2/2] package:");
        let mut packaged = 0usize;
        for name in &plan.order {
            let version = version_of(&workspace, name);
            match publish::package_crate(&runner, &workspace.root, name) {
                Ok(()) => {
                    packaged += 1;
                    println!("  {name:<28} {version:<8} packaged");
                }
                Err(error) => {
                    println!("  {name:<28} {version:<8} failed: {error}");
                    println!("        packaged: {packaged}/{}", plan.order.len());
                    println!("        next: fix error, then `relay publish --dry-run`");
                    return Ok(1);
                }
            }
        }
        println!("packaged: {packaged}/{}", plan.order.len());
        return Ok(0);
    }

    println!("[2/3] version check:");
    let probe = CratesIoProbe;
    let git = publish::git_tags(&runner, &workspace.root);
    let mut published_names = BTreeSet::new();
    for name in &plan.order {
        let version = workspace.require(name)?.version.clone();
        let check = publish::check_already_published(&probe, git.as_ref(), name, &version);
        for warning in &check.warnings {
            eprintln!("warning: {warning}");
        }
        if check.published {
            published_names.insert(name.clone());
            println!("  {name:<28} {version:<8} skipped (already published)");
        } else {
            println!("  {name:<28} {version:<8} not published ✓");
        }
    }
    let (pending, already_published) = partition_pending(&plan.order, &published_names);
    if pending.is_empty() {
        println!("Nothing to publish: all crates are already published.");
        return Ok(0);
    }
    if already_published > 0 {
        println!("skipped already published: {already_published}");
    }

    println!("[3/3] publish:");
    let retry = RetryConfig::from_env();
    let sleeper = SystemSleeper;
    let mut published = 0usize;
    for name in &pending {
        let version = version_of(&workspace, name);
        match publish::publish_with_retries(
            &runner,
            &sleeper,
            &workspace.root,
            name,
            &invocation,
            &retry,
        ) {
            Ok(attempt) if attempt.success => {
                published += 1;
                let suffix = if attempt.retries > 0 {
                    format!(" (retry {})", attempt.retries)
                } else {
                    String::new()
                };
                println!("  {name:<28} {version:<8} published{suffix}");
            }
            Ok(attempt) => {
                println!(
                    "  {name:<28} {version:<8} failed: {}",
                    stderr_tail(&attempt.stderr, 400)
                );
                println!("        published: {published}/{}", pending.len());
                println!("        next: fix error, then `relay publish {name}`");
                return Ok(1);
            }
            Err(error) => {
                println!("  {name:<28} {version:<8} failed: {error}");
                println!("        published: {published}/{}", pending.len());
                println!("        next: fix error, then `relay publish {name}`");
                return Ok(1);
            }
        }
    }
    println!("published: {published}/{}", pending.len());
    Ok(0)
}

fn partition_pending(order: &[String], published: &BTreeSet<String>) -> (Vec<String>, usize) {
    let mut pending = Vec::new();
    let mut skipped = 0usize;
    for name in order {
        if published.contains(name) {
            skipped += 1;
        } else {
            pending.push(name.clone());
        }
    }
    (pending, skipped)
}

fn version_of(workspace: &Workspace, name: &str) -> String {
    workspace
        .package(name)
        .map(|package| package.version.to_string())
        .unwrap_or_else(|| "?".to_string())
}

fn stderr_tail(stderr: &str, max: usize) -> String {
    let trimmed = stderr.trim();
    if trimmed.len() <= max {
        return trimmed.to_string();
    }
    let start = trimmed.len() - max;
    let start = (start..trimmed.len())
        .find(|index| trimmed.is_char_boundary(*index))
        .unwrap_or(trimmed.len());
    format!("…{}", &trimmed[start..])
}

#[cfg(test)]
mod tests {
    use super::partition_pending;
    use std::collections::BTreeSet;

    #[test]
    fn partition_pending_skips_already_published() {
        let order = vec!["relay-html".to_string(), "relay-core".to_string()];
        let published = BTreeSet::from(["relay-html".to_string()]);
        let (pending, skipped) = partition_pending(&order, &published);
        assert_eq!(pending, vec!["relay-core"]);
        assert_eq!(skipped, 1);
    }

    #[test]
    fn partition_pending_returns_empty_when_all_published() {
        let order = vec!["relay-html".to_string()];
        let published = BTreeSet::from(["relay-html".to_string()]);
        let (pending, skipped) = partition_pending(&order, &published);
        assert!(pending.is_empty());
        assert_eq!(skipped, 1);
    }
}
