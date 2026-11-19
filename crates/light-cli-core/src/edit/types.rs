use toml_edit::{Array, DocumentMut, InlineTable, Item, Table, Value};

use crate::error::{Error, Result};

/// Разбирает текст `Cargo.toml`.
pub fn parse(text: &str) -> Result<DocumentMut> {
    text.parse::<DocumentMut>()
        .map_err(|e| Error::Manifest(format!("не удалось разобрать Cargo.toml: {e}")))
}

/// Внутренний: таблица `[dependencies]`.
pub(crate) fn dependencies_table(doc: &mut DocumentMut) -> Result<&mut Table> {
    doc.entry("dependencies")
        .or_insert_with(|| Item::Table(Table::new()));
    doc.get_mut("dependencies")
        .and_then(Item::as_table_mut)
        .ok_or_else(|| Error::Manifest("секция [dependencies] не является таблицей".to_string()))
}

/// Внутренний: таблица `[features]`.
pub(crate) fn features_table(doc: &mut DocumentMut) -> Result<&mut Table> {
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

/// Есть ли зависимость с таким именем в `[dependencies]`.
pub fn has_dependency(doc: &DocumentMut, name: &str) -> bool {
    doc.get("dependencies")
        .and_then(Item::as_table_like)
        .is_some_and(|table| table.contains_key(name))
}

/// Добавляет фичи в существующую зависимость, не дублируя их.
///
/// Строковая форма `name = "1.0"` превращается в
/// `name = { version = "1.0", features = [...] }`.
pub fn add_features_to_dependency(
    doc: &mut DocumentMut,
    name: &str,
    features: &[String],
) -> Result<()> {
    if features.is_empty() {
        return Ok(());
    }
    let entry = dependencies_table(doc)?
        .get_mut(name)
        .ok_or_else(|| Error::Manifest(format!("зависимость `{name}` не найдена")))?;

    let table = if entry.is_str() {
        let version = entry.as_str().unwrap_or_default().to_string();
        let mut inline = InlineTable::new();
        inline.insert("version", Value::from(version));
        inline.insert("features", Value::Array(Array::new()));
        *entry = Item::Value(Value::InlineTable(inline));
        entry
            .as_table_like_mut()
            .expect("только что создали inline-таблицу")
    } else {
        entry.as_table_like_mut().ok_or_else(|| {
            Error::Manifest(format!("не удалось добавить фичи зависимости `{name}`"))
        })?
    };

    if table.get_mut("features").is_none() {
        table.insert("features", Item::Value(Value::Array(Array::new())));
    }
    let array = table
        .get_mut("features")
        .and_then(Item::as_array_mut)
        .ok_or_else(|| Error::Manifest(format!("features зависимости `{name}` не массив")))?;
    for feature in features {
        if !array
            .iter()
            .any(|value| value.as_str() == Some(feature.as_str()))
        {
            array.push(feature.clone());
        }
    }
    if let Item::Value(Value::InlineTable(inline)) = entry {
        inline.fmt();
    }
    Ok(())
}

/// Точка вставки нового `dep:`-элемента в **многострочном** массиве и его
/// префикс (стиль соседа). Возвращает `None` для однострочных массивов.
///
/// Элемент вставляется **после последнего `dep:`**, чтобы группы зависимостей
/// не разъезжались и повторный `remove`+`add` давал byte-perfect roundtrip.
/// Внутренний: точка вставки `dep:`-элемента в многострочном массиве.
pub(crate) fn multiline_dep_insertion(arr: &Array) -> Option<(usize, String)> {
    if !arr.to_string().contains('\n') {
        return None;
    }
    let after = arr
        .iter()
        .enumerate()
        .filter(|(_, v)| v.as_str().is_some_and(|s| s.starts_with("dep:")))
        .map(|(i, _)| i + 1)
        .last()
        .unwrap_or_else(|| arr.iter().count());
    let neighbor = arr
        .iter()
        .nth(after.saturating_sub(1))
        .or_else(|| arr.iter().last());
    let prefix = neighbor
        .and_then(|v| v.decor().prefix().and_then(|p| p.as_str()))
        .map(|prefix| match prefix.rfind('\n') {
            // Отбрасываем комментарии соседа: берём только перевод строки и
            // отступ, иначе при вставке комментарий продублируется.
            Some(index) => &prefix[index..],
            None => prefix,
        })
        .filter(|prefix| prefix.contains('\n'))
        .unwrap_or("\n    ")
        .to_string();
    Some((after, prefix))
}
