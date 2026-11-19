//! Каталог компонентов: где искать метаданные крейта-компонента.
//!
//! Разрешение идёт в порядке: **workspace** → **vendor** → **crates.io**.
//! Реальные сетевые запросы спрятаны за фичей `network`; без неё crates.io
//! заменяется встроенным реестром (`registry.toml`), поэтому ядро CLI
//! собирается и тестируется без сети.

/// Каталог разрешения компонентов для проекта.
pub mod catalog;
/// Вспомогательные функции и запись зависимостей.
pub mod helpers;
/// Сетевые запросы к crates.io (за фичей `network`).
#[cfg(feature = "network")]
pub mod http;
/// Типы данных и источник разрешения.
pub mod types;

pub use catalog::*;
pub use helpers::*;
#[cfg(feature = "network")]
pub use http::*;
pub use types::*;

#[cfg(test)]
mod tests;
