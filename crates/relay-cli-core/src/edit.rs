//! Редактирование `Cargo.toml` с сохранением форматирования.
//!
//! Для зависимостей используется `cargo add`/`cargo remove`, но когда это
//! невозможно (зависимость из `[workspace.dependencies]`, которую `cargo add`
//! не умеет записывать как `workspace = true`), правка выполняется здесь через
//! `toml_edit`. Секция `[features]` редактируется только здесь.

use toml_edit::{Array, DocumentMut, InlineTable, Item, Table, Value, value};

use crate::error::{Error, Result};

/// Разбирает текст `Cargo.toml`.
pub fn parse(text: &str) -> Result<DocumentMut> {
    text.parse::<DocumentMut>()
        .map_err(|e| Error::Manifest(format!("не удалось разобрать Cargo.toml: {e}")))
}

fn dependencies_table(doc: &mut DocumentMut) -> Result<&mut Table> {
    doc.entry("dependencies")
        .or_insert_with(|| Item::Table(Table::new()));
    doc.get_mut("dependencies")
        .and_then(Item::as_table_mut)
        .ok_or_else(|| Error::Manifest("секция [dependencies] не является таблицей".to_string()))
}

fn features_table(doc: &mut DocumentMut) -> Result<&mut Table> {
    doc.entry("features")
        .or_insert_with(|| Item::Table(Table::new()));
    doc.get_mut("features")
        .and_then(Item::as_table_mut)
        .ok_or_else(|| Error::Manifest("секция [features] не является таблицей".to_string()))
}

fn optional_dep(kind: (&str, &str), optional: bool) -> Item {
    let mut inline = InlineTable::new();
    inline.insert(kind.0, Value::from(kind.1));
    inline.insert("optional", Value::from(optional));
    Item::Value(Value::InlineTable(inline))
}

/// Записывает `name = { workspace = true, optional = true }`.
pub fn set_workspace_optional(doc: &mut DocumentMut, name: &str) -> Result<()> {
    let mut inline = InlineTable::new();
    inline.insert("workspace", Value::from(true));
    inline.insert("optional", Value::from(true));
    dependencies_table(doc)?.insert(name, Item::Value(Value::InlineTable(inline)));
    Ok(())
}

/// Записывает `name = { path = "...", optional = true }`.
pub fn set_path_optional(doc: &mut DocumentMut, name: &str, path: &str) -> Result<()> {
    dependencies_table(doc)?.insert(name, optional_dep(("path", path), true));
    Ok(())
}

/// Записывает `name = { version = "...", optional = true }`.
pub fn set_version_optional(doc: &mut DocumentMut, name: &str, version: &str) -> Result<()> {
    dependencies_table(doc)?.insert(name, optional_dep(("version", version), true));
    Ok(())
}

/// Добавляет `dep:name` в фичу. Возвращает `true`, если фича была создана.
pub fn add_dep_to_feature(doc: &mut DocumentMut, feature: &str, name: &str) -> Result<bool> {
    let table = features_table(doc)?;
    let existed = table.contains_key(feature);
    let entry = table.entry(feature).or_insert_with(|| value(Array::new()));
    let arr = entry
        .as_array_mut()
        .ok_or_else(|| Error::Manifest(format!("фича `{feature}` не является массивом")))?;
    let token = format!("dep:{name}");
    if !arr.iter().any(|v| v.as_str() == Some(token.as_str())) {
        arr.push(token);
    }
    Ok(!existed)
}

/// Меняет версию зависимости: строку `name = "0.1"` или поле `version`.
pub fn set_dependency_version(doc: &mut DocumentMut, name: &str, version: &str) -> Result<()> {
    let entry = dependencies_table(doc)?
        .get_mut(name)
        .ok_or_else(|| Error::Manifest(format!("зависимость `{name}` не найдена")))?;
    if entry.is_str() {
        *entry = Item::Value(Value::from(version));
        return Ok(());
    }
    if let Some(table) = entry.as_table_like_mut() {
        table.insert("version", Item::Value(Value::from(version)));
        return Ok(());
    }
    Err(Error::Manifest(format!(
        "не удалось обновить версию зависимости `{name}`"
    )))
}

/// Есть ли фича с таким именем.
pub fn has_feature(text: &str, feature: &str) -> Result<bool> {
    let doc = parse(text)?;
    Ok(doc
        .get("features")
        .and_then(Item::as_table_like)
        .map(|t| t.contains_key(feature))
        .unwrap_or(false))
}

/// Фичи, ссылающиеся на `dep:name`.
pub fn features_containing(text: &str, name: &str) -> Result<Vec<String>> {
    let doc = parse(text)?;
    let token = format!("dep:{name}");
    let mut out = Vec::new();
    if let Some(features) = doc.get("features").and_then(Item::as_table_like) {
        for (key, item) in features.iter() {
            if let Some(arr) = item.as_array()
                && arr.iter().any(|v| v.as_str() == Some(token.as_str()))
            {
                out.push(key.to_string());
            }
        }
    }
    out.sort();
    Ok(out)
}

/// Фичи, включающие какой-либо компонент Relay (`dep:relay-*`).
///
/// Используется, чтобы подключать новый компонент в существующую
/// агрегирующую фичу (например, `full`), а не плодить отдельные.
pub fn features_with_relay_component_dep(text: &str) -> Result<Vec<String>> {
    let doc = parse(text)?;
    let mut out = Vec::new();
    if let Some(features) = doc.get("features").and_then(Item::as_table_like) {
        for (key, item) in features.iter() {
            if let Some(arr) = item.as_array()
                && arr
                    .iter()
                    .any(|v| v.as_str().is_some_and(|s| s.starts_with("dep:relay-")))
            {
                out.push(key.to_string());
            }
        }
    }
    out.sort();
    Ok(out)
}

/// Удаляет зависимость `name` и её `dep:`-токены из всех фич.
///
/// Возвращает имена затронутых фич. Пустые (кроме `default`) фичи удаляются.
pub fn remove_dependency(doc: &mut DocumentMut, name: &str) -> Result<Vec<String>> {
    if let Some(deps) = doc.get_mut("dependencies").and_then(Item::as_table_mut) {
        deps.remove(name);
    }
    let mut touched = Vec::new();
    let token = format!("dep:{name}");
    let Some(features) = doc.get_mut("features").and_then(Item::as_table_mut) else {
        return Ok(touched);
    };
    let keys: Vec<String> = features.iter().map(|(k, _)| k.to_string()).collect();
    for key in keys {
        let Some(arr) = features.get_mut(&key).and_then(Item::as_array_mut) else {
            continue;
        };
        let before = arr.len();
        let mut i = 0;
        while i < arr.len() {
            if arr.get(i).and_then(Value::as_str) == Some(token.as_str()) {
                arr.remove(i);
            } else {
                i += 1;
            }
        }
        if arr.len() != before {
            touched.push(key.clone());
        }
        if arr.is_empty() && key != "default" {
            features.remove(&key);
        }
    }
    Ok(touched)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_workspace_optional_dependency() {
        let mut doc =
            parse("[package]\nname = \"p\"\n\n[dependencies]\nrelay = \"0.1\"\n").unwrap();
        set_workspace_optional(&mut doc, "relay-element-form").unwrap();
        let out = doc.to_string();
        assert!(out.contains("relay-element-form = { workspace = true, optional = true }"));
        assert!(out.contains("relay = \"0.1\""), "форматирование сохранено");
    }

    #[test]
    fn adds_version_optional_dependency() {
        let mut doc = parse("[package]\nname = \"p\"\n").unwrap();
        set_version_optional(&mut doc, "relay-element-form", "0.1.0").unwrap();
        assert!(
            doc.to_string()
                .contains("relay-element-form = { version = \"0.1.0\", optional = true }")
        );
    }

    #[test]
    fn adds_dep_to_existing_and_new_feature() {
        let mut doc = parse("[features]\ndefault = [\"full\"]\nfull = [\"relay/x\"]\n").unwrap();
        assert!(!add_dep_to_feature(&mut doc, "full", "relay-element-form").unwrap());
        assert!(add_dep_to_feature(&mut doc, "element-form", "relay-element-form").unwrap());
        assert!(!add_dep_to_feature(&mut doc, "full", "relay-element-form").unwrap());
        let out = doc.to_string();
        assert!(out.contains("\"dep:relay-element-form\""));
        assert!(out.contains("element-form = [\"dep:relay-element-form\"]"));
    }

    #[test]
    fn removes_dependency_and_feature_tokens() {
        let mut doc = parse(
            "[dependencies]\nrelay = \"0.1\"\nrelay-element-form = { workspace = true, optional = true }\n\n\
             [features]\ndefault = [\"full\"]\nfull = [\"dep:relay-element-form\", \"relay/x\"]\n\
             element-form = [\"dep:relay-element-form\"]\n",
        )
        .unwrap();
        let touched = remove_dependency(&mut doc, "relay-element-form").unwrap();
        let mut sorted = touched.clone();
        sorted.sort();
        assert_eq!(sorted, vec!["element-form", "full"]);
        let out = doc.to_string();
        assert!(!out.contains("relay-element-form"));
        assert!(!out.contains("dep:"));
        assert!(out.contains("relay/x"));
        assert!(out.contains("default"));
        assert!(out.contains("full"));
    }

    #[test]
    fn sets_dependency_version_string_and_table() {
        let mut doc = parse(
            "[dependencies]\nrelay-element-form = \"0.0.1\"\nrelay = { version = \"0.1\", features = [\"x\"] }\n",
        )
        .unwrap();
        set_dependency_version(&mut doc, "relay-element-form", "0.1.0").unwrap();
        set_dependency_version(&mut doc, "relay", "0.2.0").unwrap();
        let out = doc.to_string();
        assert!(out.contains("relay-element-form = \"0.1.0\""));
        assert!(out.contains("version = \"0.2.0\""));
        assert!(
            out.contains("features = [\"x\"]"),
            "features сохранены: {out}"
        );
    }

    #[test]
    fn features_containing_reports_matches() {
        let text = "[features]\nfull = [\"dep:relay-element-form\"]\nminimal = []\n";
        assert_eq!(
            features_containing(text, "relay-element-form").unwrap(),
            vec!["full"]
        );
        assert!(has_feature(text, "full").unwrap());
        assert!(!has_feature(text, "nope").unwrap());
    }
}
