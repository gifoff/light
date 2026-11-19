use std::path::Path;

use crate::error::{Error, Result};
use crate::process::CommandRunner;

/// Результат одного гейта pre-flight.
#[derive(Debug, Clone)]
pub struct GateOutcome {
    /// Имя гейта (`build`/`test`/`clippy`/`fmt`).
    pub name: String,
    /// Успешно ли.
    pub ok: bool,
    /// stderr при неудаче.
    pub stderr: String,
}

/// Гейты pre-flight с опциональным исключением пакета.
///
/// `exclude` — имя пакета, который не нужно пересобирать (например, только что
/// запущенный бинарник `light`); `cargo fmt` исключение не поддерживает.
pub fn gate_commands(exclude: Option<&str>) -> Vec<(&'static str, Vec<String>)> {
    fn args(base: &[&str], exclude: Option<&str>) -> Vec<String> {
        let mut args: Vec<String> = base.iter().map(|arg| (*arg).to_string()).collect();
        if let Some(name) = exclude {
            args.push("--exclude".to_string());
            args.push(name.to_string());
        }
        args
    }

    let mut clippy = args(&["clippy", "--workspace"], exclude);
    clippy.push("--".to_string());
    clippy.push("-D".to_string());
    clippy.push("warnings".to_string());

    vec![
        ("build", args(&["build", "--workspace"], exclude)),
        ("test", args(&["test", "--workspace"], exclude)),
        ("clippy", clippy),
        ("fmt", args(&["fmt", "--check", "--all"], None)),
    ]
}

/// Прогоняет гейты `build`/`test`/`clippy`/`fmt`, останавливаясь на первом сбое.
pub fn run_gates(
    runner: &dyn CommandRunner,
    root: &Path,
    exclude: Option<&str>,
) -> Result<Vec<GateOutcome>> {
    let mut outcomes = Vec::new();
    for (name, args) in gate_commands(exclude) {
        let output = runner.run("cargo", &args, Some(root))?;
        let ok = output.success();
        outcomes.push(GateOutcome {
            name: name.to_string(),
            ok,
            stderr: output.stderr.trim().to_string(),
        });
        if !ok {
            break;
        }
    }
    Ok(outcomes)
}

/// Аргументы `cargo package --list` для проверки упаковки в режиме dry-run.
pub fn package_list_args(name: &str) -> Vec<String> {
    vec![
        "package".to_string(),
        "--list".to_string(),
        "-p".to_string(),
        name.to_string(),
        "--allow-dirty".to_string(),
    ]
}

/// Проверяет упаковку крейта через `cargo package --list`.
pub fn package_crate(runner: &dyn CommandRunner, root: &Path, name: &str) -> Result<()> {
    let output = runner.run("cargo", &package_list_args(name), Some(root))?;
    if !output.success() {
        return Err(Error::CommandFailed {
            program: format!("cargo package -p {name}"),
            code: output.status,
            stderr: output.stderr.trim().to_string(),
        });
    }
    Ok(())
}

/// Параметры вызова `cargo publish`.
#[derive(Debug, Clone, Default)]
pub struct PublishInvocation {
    /// Полная верификация (без `--no-verify`).
    pub verify: bool,
    /// Разрешить грязное рабочее дерево.
    pub allow_dirty: bool,
    /// Токен реестра.
    pub token: Option<String>,
}

/// Аргументы `cargo publish` для крейта.
pub fn publish_args(name: &str, invocation: &PublishInvocation) -> Vec<String> {
    let mut args = vec!["publish".to_string(), "-p".to_string(), name.to_string()];
    if !invocation.verify {
        args.push("--no-verify".to_string());
    }
    if invocation.allow_dirty {
        args.push("--allow-dirty".to_string());
    }
    if let Some(token) = &invocation.token {
        args.push("--token".to_string());
        args.push(token.clone());
    }
    args
}
