use std::collections::BTreeMap;
use std::io::{self, IsTerminal};

use super::types::ComponentInfo;
use crate::manifest::Provides;

pub(crate) fn is_interactive() -> bool {
    io::stdin().is_terminal()
}

/// в этот список, сопоставляется с установленными компонентами и встроенным
/// реестром.
pub(crate) const STANDARD_TAGS: &[&str] = &[
    // Виртуальные теги Light (корневые, не из стандартного HTML, но
    // поддерживаются ядром без какого-либо компонента). `image` — PNG-изображение
    // (фича `image-support`); это **ядро**, а не внешний компонент, поэтому его
    // отсутствие из списка приводило к ложному «неизвестный тег» в `light dev`/
    // `light check` (регрессия 0.20.2).
    "window", "text", "password", "image", // HTML5-строка.
    "html", "head", "body", "div", "span", "p", "a", "ul", "ol", "li", "h1", "h2", "h3", "h4",
    "h5", "h6", "button", "input", "label", "form", "table", "tr", "td", "th", "img", "video",
    "audio", "canvas", "svg", "section", "article", "header", "footer", "nav", "main", "aside",
    "dialog", "details", "summary", "select", "option", "textarea", "pre", "code", "strong", "em",
    "b", "i", "small", "sup", "sub", "hr", "br", "wbr", "u", "s", "strike",
];

/// Установка: стандартные теги не требуют компонентов.
pub(crate) fn is_standard_tag(tag: &str) -> bool {
    STANDARD_TAGS.contains(&tag)
}

/// Собирает в `out` имена всех тегов вида `<имя ...>` из текста HTML.
pub(crate) fn extract_tag_names(text: &str, out: &mut BTreeMap<String, usize>) {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        // После '<' идём дальше; первый символ имени — буква.
        i += 1;
        if i >= bytes.len() || !(bytes[i].is_ascii_alphabetic()) {
            continue;
        }
        let start = i;
        while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-') {
            i += 1;
        }
        let name = std::str::from_utf8(&bytes[start..i]).unwrap_or("");
        if !name.is_empty() {
            *out.entry(name.to_string()).or_insert(0) += 1;
        }
    }
}

/// Преобразует метаданные из `cargo metadata` в компактный [`ComponentInfo`].
pub(crate) fn meta_to_info(meta: crate::metadata::ComponentMetadata) -> ComponentInfo {
    ComponentInfo {
        crate_name: meta.crate_name,
        version: meta.version,
        provides: Provides {
            elements: meta.provides_elements,
            attributes: meta.provides_attributes,
            directives: meta.provides_directives,
        },
    }
}

/// Использовано ли свойство/директива в шаблонах. Атрибут может встречаться с
/// префиксами (`:value`, `@validate`, `v-model`) — сравниваем как целиком, так
/// и после снятия префикса.
pub(crate) fn attr_used(used: &BTreeMap<String, usize>, value: &str) -> bool {
    used.keys()
        .any(|k| k == value || k.trim_start_matches([':', '@', 'v']) == value)
}
