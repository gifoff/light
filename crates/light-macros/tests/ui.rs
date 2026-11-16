//! Trybuild-тесты: проверяют `compile_error!` на неизвестные кастомные теги
//! в HTML-шаблонах (ТЗ 0.19.0 Этап 4, Plugin API).
//!
//! Успешный кейс (`component_ok`) — *внутри* текущего крейта (юнит-тест
//! `component.rs::generates_custom_element_impl`), а не через trybuild:
//! trybuild компилируется в контексте `light-fw-macros`, где
//! `[package.metadata.light.provides]` пуст, поэтому любые кастомные теги
//! (включая `<datepicker>` с `#[component]`) дадут `compile_error!`.

#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    // `#[template]` с неизвестным тегом → compile_error «неизвестный элемент».
    t.compile_fail("ui/component_unknown_tag.rs");
}
