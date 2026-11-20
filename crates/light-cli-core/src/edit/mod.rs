//! Редактирование `Cargo.toml` с сохранением форматирования.
//!
//! Для зависимостей используется `cargo add`/`cargo remove`, но когда это
//! невозможно (зависимость из `[workspace.dependencies]`, которую `cargo add`
//! не умеет записывать как `workspace = true`), правка выполняется здесь через
//! `toml_edit`. Секция `[features]` редактируется только здесь.

/// Операции над `[features]`: добавление/удаление фич и `dep:`-токенов.
pub mod features;
#[cfg(test)]
mod tests;
/// Базовые операции над `[dependencies]`: парсинг и запись зависимостей.
pub mod types;

pub use features::*;
pub use types::*;
