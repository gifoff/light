//! Trybuild-тесты: проверяют `compile_error!` на неизвестные кастомные теги
//! в HTML-шаблонах (ТЗ 0.19.0 Этап 4, Plugin API).
//!
//! Успешный кейс (`component_ok`) — внутри `custom_elements.rs`.
//! Неудачный кейс (неизвестный тег) — через trybuild: крейт без
//! `[package.metadata.light.provides]` не может использовать нестандартные
//! теги, и `#[template]` выдаёт `compile_error!` с подсказкой.

#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    // `#[template]` с неизвестным тегом → compile_error «неизвестный элемент».
    t.compile_fail("tests/ui/component_unknown_tag.rs");
}
