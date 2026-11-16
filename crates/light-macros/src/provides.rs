//! Чтение `[package.metadata.light]` из `Cargo.toml` текущего пакета.
//!
//! Используется на этапе компиляции `#[template]`: теги, объявленные в
//! `[package.metadata.light.provides.elements]`, разрешены в HTML-шаблонах
//! (они рендерятся как `NodeKind::Custom` через плагинную фабрику), а все
//! прочие нестандартные теги — ошибка компиляции (ТЗ 0.19.0 Этап 4).
//!
//! Семантика «умершего» манифеста: если `CARGO_MANIFEST_DIR` не задан,
//! `Cargo.toml` отсутствует или бит (непарсебый TOML) — предоставляет
//! *пустой* набор кастомных тегов. Стандартные элементы при этом продолжают
//! работать; неизвестный тег даст понятный `compile_error!`.

use std::collections::HashSet;
use std::path::Path;

/// Имя секции метаданных Light в `Cargo.toml`.
const LIGHT_SECTION: &str = "light";

/// Читает `[package.metadata.light.provides.elements]` из манифеста пакета.
///
/// `manifest_dir` — путь к корню пакета (обычно `CARGO_MANIFEST_DIR`).
/// Возвращает множество объявленных кастомных тегов; при любом сбое — пустое
/// множество (см. модуль).
pub fn read_provides_elements(manifest_dir: &Path) -> HashSet<String> {
    let path = manifest_dir.join("Cargo.toml");
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(_) => return HashSet::new(),
    };
    parse_provides_elements(&text)
}

/// Парсит текст `Cargo.toml` и возвращает `[package.metadata.light.provides.elements]`.
///
/// Выделен для юнит-тестов (битый TOML, отсутствующие секции). При любом
/// ошибке парсинга возвращается пустой набор.
pub fn parse_provides_elements(text: &str) -> HashSet<String> {
    let value: toml::Value = match text.parse() {
        Ok(value) => value,
        Err(_) => return HashSet::new(),
    };
    let elements = value
        .get("package")
        .and_then(|package| package.get("metadata"))
        .and_then(|metadata| metadata.get(LIGHT_SECTION))
        .and_then(|light| light.get("provides"))
        .and_then(|provides| provides.get("elements"));
    let Some(elements) = elements else {
        return HashSet::new();
    };
    match elements {
        toml::Value::Array(items) => items
            .iter()
            .filter_map(|item| item.as_str())
            .map(ToString::to_string)
            .collect(),
        _ => HashSet::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_elements_list() {
        let text = r#"
[package]
name = "demo"
version = "0.1.0"

[package.metadata.light]
core = "^0.18"

[package.metadata.light.provides]
elements = ["datepicker", "calendar"]
"#;
        let set = parse_provides_elements(text);
        assert_eq!(set.len(), 2);
        assert!(set.contains("datepicker"));
        assert!(set.contains("calendar"));
    }

    #[test]
    fn missing_metadata_gives_empty_set() {
        let text = r#"
[package]
name = "plain"
version = "0.1.0"
"#;
        assert!(parse_provides_elements(text).is_empty());
    }

    #[test]
    fn malformed_toml_gives_empty_set() {
        let text = "[package\nname = ???";
        assert!(parse_provides_elements(text).is_empty());
    }

    #[test]
    fn non_array_elements_gives_empty_set() {
        let text = r#"
[package]
name = "demo"

[package.metadata.light.provides]
elements = "datepicker"
"#;
        assert!(parse_provides_elements(text).is_empty());
    }

    #[test]
    fn mixed_type_items_are_filtered() {
        let text = r#"
[package]
name = "demo"

[package.metadata.light.provides]
elements = ["ok", 42, true]
"#;
        let set = parse_provides_elements(text);
        assert_eq!(set, HashSet::from(["ok".to_string()]));
    }
}
