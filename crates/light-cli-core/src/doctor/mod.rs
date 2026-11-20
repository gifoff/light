//! Команда `light doctor`: диагностика окружения и проекта.
//!
//! Модуль только читает данные (файлы, версии инструментов, сеть) и ничего не
//! меняет на диске. Внешние зависимости (запуск `rustc`/`cargo`/`rustup` и
//! доступность сети) инжектируются через [`EnvironmentProbe`], поэтому логика
//! тестируется без реальных процессов.

/// Функции проверки: doctor(), check_environment, check_project.
pub mod checks;
/// Пробник системного окружения (версии rustc/cargo/rustup, сеть).
pub mod probe;
/// Типы отчёта: Severity, DoctorIssue, секции, DoctorReport.
pub mod types;

pub use checks::*;
pub use probe::*;
pub use types::*;

#[cfg(test)]
mod tests;
