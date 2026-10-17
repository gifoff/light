//! Обёртка над подкомандами `cargo`.
//!
//! Здесь формируются векторы аргументов (чтобы их можно было проверять в
//! тестах без запуска процесса) и выполняются вызовы `cargo add` / `cargo
//! remove` / `cargo check` через [`CommandRunner`].

use std::path::Path;

use crate::error::{Error, Result};
use crate::process::CommandRunner;

/// Аргументы `cargo add`.
///
/// Для workspace-члена передаётся `path` (тогда используется `--path`), иначе
/// `name@version` при наличии версии.
pub fn add_args(
    name: &str,
    version: Option<&str>,
    path: Option<&Path>,
    optional: bool,
) -> Vec<String> {
    let mut args = vec!["add".to_string()];
    if let Some(p) = path {
        args.push(name.to_string());
        args.push("--path".to_string());
        args.push(p.display().to_string());
    } else if let Some(v) = version {
        args.push(format!("{name}@{v}"));
    } else {
        args.push(name.to_string());
    }
    if optional {
        args.push("--optional".to_string());
    }
    args
}

/// Аргументы `cargo remove`.
pub fn remove_args(name: &str) -> Vec<String> {
    vec!["remove".to_string(), name.to_string()]
}

/// Аргументы `cargo check`.
pub fn check_args() -> Vec<String> {
    vec!["check".to_string(), "--quiet".to_string()]
}

/// Запускает `cargo add` и проверяет код возврата.
pub fn run_add(
    runner: &dyn CommandRunner,
    cwd: &Path,
    name: &str,
    version: Option<&str>,
    path: Option<&Path>,
    optional: bool,
) -> Result<()> {
    run_cargo(runner, cwd, &add_args(name, version, path, optional))
}

/// Запускает `cargo remove` и проверяет код возврата.
pub fn run_remove(runner: &dyn CommandRunner, cwd: &Path, name: &str) -> Result<()> {
    run_cargo(runner, cwd, &remove_args(name))
}

/// Запускает `cargo check` в каталоге проекта.
pub fn run_check(runner: &dyn CommandRunner, cwd: &Path, no_verify: bool) -> Result<()> {
    if no_verify {
        return Ok(());
    }
    run_cargo(runner, cwd, &check_args())
}

fn run_cargo(runner: &dyn CommandRunner, cwd: &Path, args: &[String]) -> Result<()> {
    let output = runner.run("cargo", args, Some(cwd))?;
    if !output.success() {
        return Err(Error::CommandFailed {
            program: format!("cargo {}", args.first().map(String::as_str).unwrap_or("")),
            code: output.status,
            stderr: output.stderr.trim().to_string(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn add_args_with_version() {
        assert_eq!(
            add_args("relay-element-form", Some("0.1.2"), None, true),
            vec!["add", "relay-element-form@0.1.2", "--optional"]
        );
    }

    #[test]
    fn add_args_with_path() {
        let path = PathBuf::from("crates/relay-element-form");
        assert_eq!(
            add_args("relay-element-form", None, Some(path.as_path()), true),
            vec![
                "add",
                "relay-element-form",
                "--path",
                "crates/relay-element-form",
                "--optional"
            ]
        );
    }

    #[test]
    fn add_args_without_version() {
        assert_eq!(
            add_args("relay-widget-datepicker", None, None, false),
            vec!["add", "relay-widget-datepicker"]
        );
    }

    #[test]
    fn remove_args_shape() {
        assert_eq!(
            remove_args("relay-element-form"),
            vec!["remove", "relay-element-form"]
        );
    }
}
