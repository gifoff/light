use toml_edit::{Array, DocumentMut, Item, Value, value};

use crate::error::{Error, Result};

use super::types::{dependencies_table, features_table, multiline_dep_insertion, parse};

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
        match multiline_dep_insertion(arr) {
            Some((index, prefix)) => {
                let mut new_value = Value::from(token);
                new_value.decor_mut().set_prefix(prefix);
                // `Array::insert` перезаписывает декорации значения на `" "`/`""`,
                // поэтому используем `insert_formatted`, сохраняющий префикс.
                arr.insert_formatted(index, new_value);
            }
            None => arr.push(token),
        }
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

/// Фичи, включающие какой-либо компонент Light (`dep:light-*`).
///
/// Используется, чтобы подключать новый компонент в существующую
/// агрегирующую фичу (например, `full`), а не плодить отдельные.
pub fn features_with_light_component_dep(text: &str) -> Result<Vec<String>> {
    let doc = parse(text)?;
    let mut out = Vec::new();
    if let Some(features) = doc.get("features").and_then(Item::as_table_like) {
        for (key, item) in features.iter() {
            if let Some(arr) = item.as_array()
                && arr
                    .iter()
                    .any(|v| v.as_str().is_some_and(|s| s.starts_with("dep:light-")))
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
/// Возвращает имена затронутых фич. Фичи, опустевшие в результате удаления
/// (кроме `default`), также удаляются; изначально пустые фичи сохраняются.
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
        if arr.len() != before && arr.is_empty() && key != "default" {
            features.remove(&key);
        }
    }
    Ok(touched)
}
