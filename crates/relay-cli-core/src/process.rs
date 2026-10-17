//! Обёртка над запуском внешних процессов.
//!
//! Реальная работа с `cargo`/`rustc`/`rustup` идёт через трейт [`CommandRunner`],
//! что позволяет тестировать логику CLI без запуска настоящих процессов
//! (в тестах подставляется мок-раннер).

use std::path::Path;
use std::process::Command;

use crate::error::{Error, Result};

/// Результат запуска внешней команды.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    /// Код возврата (`-1`, если процесс завершён сигналом).
    pub status: i32,
    /// Стандартный вывод.
    pub stdout: String,
    /// Стандартный поток ошибок.
    pub stderr: String,
}

impl CommandOutput {
    /// Успешно ли завершилась команда.
    pub fn success(&self) -> bool {
        self.status == 0
    }
}

/// Запускает внешние команды. Реализация по умолчанию — [`SystemRunner`].
pub trait CommandRunner: Send + Sync {
    /// Запускает `program` с аргументами `args` в каталоге `cwd`.
    fn run(&self, program: &str, args: &[String], cwd: Option<&Path>) -> Result<CommandOutput>;
}

/// Раннер, запускающий реальные процессы через [`std::process::Command`].
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemRunner;

impl CommandRunner for SystemRunner {
    fn run(&self, program: &str, args: &[String], cwd: Option<&Path>) -> Result<CommandOutput> {
        let mut command = Command::new(program);
        command.args(args);
        if let Some(dir) = cwd {
            command.current_dir(dir);
        }
        let output = command.output().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Error::CommandNotFound(program.to_string())
            } else {
                Error::Io(e)
            }
        })?;
        Ok(CommandOutput {
            status: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }
}

/// Извлекает semver-версию из первой строки вывода `* --version`.
///
/// Формат: `<program> <version> (<hash> <date>)`, например
/// `rustc 1.99.0 (5f94df478 2026-08-27)`.
pub fn parse_program_version(stdout: &str) -> Option<semver::Version> {
    let mut parts = stdout.split_whitespace();
    let _program = parts.next()?;
    let version = parts.next()?;
    semver::Version::parse(version).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_version_line() {
        let v =
            parse_program_version("rustc 1.99.0 (5f94df478 2026-08-27)").expect("version parsed");
        assert_eq!(v, semver::Version::new(1, 99, 0));
    }

    #[test]
    fn version_parse_rejects_garbage() {
        assert!(parse_program_version("no version here").is_none());
        assert!(parse_program_version("").is_none());
    }
}
