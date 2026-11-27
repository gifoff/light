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

/// Требование к ядру, гарантированно совместимое с [`LIGHT_CORE_VERSION`].
///
/// Формальный semver 2.0: pre-release-версия (`0.21.0-alpha.1`) **не
/// матчит** release-requirement без pre-release (`^0.21`, `^0.21.0`),
/// поэтому фикстуры не могут использовать «голый» caret — они упадут на
/// альфа-ядре с `IncompatibleCore`.
///
/// Правило:
/// - если версия pre-release → берём её release-базу и добавляем суффикс
///   `-0`, который открывает pre-release-окно вверх от минимального patch
///   (`0.21.0-alpha.1` → `^0.21.0-0` — матчит `0.21.0-alpha.1`,
///   `0.21.0-alpha.2`, …, `0.21.99`, но не `0.22.0`);
/// - если версия stable → используем `major.minor` (caret-семантика:
///   `^0.21` матчит `0.21.x`, `^1` матчит `1.x.y` — переживает любой
///   bump внутри того же major/minor).
///
/// Тесты, использующие этот хелпер, переживают любой bump версии ядра
/// (0.21.0-alpha.1 → 0.21.0-alpha.2 → 0.21.0 → 0.22.0 → 1.0.0) без
/// ручной правки фикстур.
///
/// Только для тестов: не выносим в production API, т.к. семантика
/// «совместимого req» зависит от текущей версии CLI и является detail'ом
/// тестирования, а не контрактом.
#[cfg(test)]
pub(crate) fn compatible_core_req() -> String {
    use crate::metadata::types::LIGHT_CORE_VERSION;
    if LIGHT_CORE_VERSION.contains('-') {
        // Pre-release: базовая часть до первого `-` + `-0`.
        let base = LIGHT_CORE_VERSION
            .split('-')
            .next()
            .unwrap_or(LIGHT_CORE_VERSION);
        format!("{base}-0")
    } else {
        // Stable: major.minor (caret).
        let parts: Vec<&str> = LIGHT_CORE_VERSION.split('.').collect();
        if parts.len() >= 2 {
            format!("{}.{}", parts[0], parts[1])
        } else {
            LIGHT_CORE_VERSION.to_string()
        }
    }
}

#[cfg(test)]
mod tests;
