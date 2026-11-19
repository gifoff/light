//! Разбор HTML-шаблонов Light.
//!
//! Поддерживаемый синтаксис:
//! * элементы `<div>`, `<span>`, `<h1>`…`<h5>`, `<text>`, `<button>`,
//!   `<titlebar>`, `<include>`, `<template>` (проверка имён — в `template.rs`);
//! * атрибуты в двойных или одинарных кавычках, включая директивы (`@click`, `v-model`, …);
//! * самозакрывающиеся теги `<text ... />`;
//! * комментарии `<!-- ... -->`;
//! * текст с интерполяцией свойств `{{ state.count }}`.
//!
//! Пробельные текстовые узлы между элементами отбрасываются; внутри содержимого
//! переводы строк нормализуются в пробелы, края обрезаются.

/// Типы узлов: Segment, Attr, Element, Node.
pub mod types;
/// Парсер: parse(), Parser struct+impl.
pub mod parser;
/// Вспомогательные функции: is_valid_attr_name, validate_path, split_interpolation, и др.
pub(crate) mod helpers;

pub use types::*;
pub use parser::parse;

#[cfg(test)]
mod tests;
