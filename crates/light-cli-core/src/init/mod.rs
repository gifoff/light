//! Команда `light init`: создание скелета нового проекта Light.
//!
//! Модуль генерирует минимальный проект: `Cargo.toml` с зависимостью `light-fw`,
//! самодостаточный `src/main.rs` (окно с текстом «Hello, Light!»), `.gitignore`
//! и `light.lock`. Внешние эффекты (проверка `cargo`) идут через трейт
//! [`CommandRunner`], поэтому логика тестируется без запуска процессов.

/// Константы конфигурации и версии.
pub mod config;
/// Генерация файлов: `validate_name`, `project_dir`, `cargo_toml`, `main_rs`.
pub mod generate;
/// Применение плана: `plan`, `require_cargo`, `scaffold`, `apply`.
pub mod run;
/// Типы данных для планирования и отчёта.
pub mod types;

pub use config::*;
pub use generate::*;
pub use run::*;
pub use types::*;

#[cfg(test)]
mod tests;
