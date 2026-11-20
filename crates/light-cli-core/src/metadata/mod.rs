//! Скачанные метаданные компонента и их валидация.
//!
//! Этап 0 Plugin API: после выбора результата поиска CLI скачивает `.crate`
//! (tar.gz), извлекает `[package.metadata.light]` из первого `*/Cargo.toml`
//! и проверяет совместимость с текущей версией ядра. Установка
//! (`cargo add`) отложена до Этапа 2, поэтому здесь только **валидация**.

/// Сканер установленных компонентов: load_installed(), load_installed_from_json().
pub mod installed;
/// Типы: LIGHT_CORE_VERSION, ComponentMetadata, ValidationResult, ValidationError.
pub mod types;
/// Валидация: validate_metadata().
pub mod validate;

pub use installed::{extract_cargo_toml, load_installed, load_installed_from_json};
pub use types::*;
pub use validate::validate_metadata;

#[cfg(test)]
mod tests;
