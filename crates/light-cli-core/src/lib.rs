//! Ядро CLI `light`: работа с манифестами, реестром, зависимостями и
//! шаблонами проектов без привязки к терминальному интерфейсу.

#![deny(missing_docs)]

pub mod add;
pub mod backup;
pub mod cargo;
pub mod doctor;
pub mod edit;
/// Типы ошибок CLI.
pub mod error;
pub mod fetch;
pub mod init;
pub mod lockfile;
/// Метаданные компонентов и разбор `Cargo.toml`.
pub mod manifest;
pub mod process;
/// Анализ проекта: зависимости, шаблоны и рекомендации.
pub mod project;
pub mod publish;
/// Встроенный реестр компонентов Light.
pub mod registry;
pub mod remove;
/// Формирование отчёта о состоянии проекта.
pub mod report;
/// Подбор компонентов, необходимых для шаблонов проекта.
pub mod resolve;
/// Сканирование HTML-шаблонов проекта.
pub mod scan;
pub mod search;
pub mod update;
pub mod vendor;

pub use error::{Error, Result};
