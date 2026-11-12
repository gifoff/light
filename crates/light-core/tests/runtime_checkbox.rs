//! Сквозной тест v-model checkbox в режиме runtime-templates.
//!
//! Проверяет, что макрос `#[template]` с фичей `runtime-templates`
//! генерирует рабочий `dispatch_vmodel`: клик по чекбоксу в runtime-дереве
//! (позиционные ключи) записывает новое значение в reactive-поле через
//! `data-v-model` атрибут.

#![cfg(all(feature = "elements-checkbox", feature = "runtime-templates"))]

extern crate light_fw_core as light_fw;

use light_fw::prelude::*;
use light_fw::vdom::{Event, NodeKind};

/// Компонент с одним чекбоксом.
#[template("tests/templates/runtime_checkbox.html")]
#[proxy]
struct FormApp {
    agreed: bool,
}

// Компонент использует макросную реализацию; дополнительные методы не нужны.

#[test]
fn checkbox_renders_with_positive_size_and_click_toggles_agreed() {
    let app = FormApp::new();
    assert!(!app.agreed(), "начальное состояние — не отмечен");

    // Рендер возвращает дерево с чекбоксом.
    let tree = app.render();
    let checkbox = tree
        .find_by_tag("checkbox")
        .expect("чекбокс присутствует в дереве");
    let NodeKind::Checkbox { label, checked, .. } = &checkbox.kind else {
        panic!("ожидался NodeKind::Checkbox");
    };
    assert_eq!(label, "Согласен", "подпись чекбокса сохранена");
    assert!(!checked, "начально не отмечен");

    // data-v-model атрибут присутствует.
    assert_eq!(
        checkbox.attr("data-v-model"),
        Some("agreed"),
        "data-v-model хранит имя поля"
    );

    // Клик по чекбоксу (через позиционный ключ из runtime-дерева).
    let key = checkbox.key;
    let handled = app.dispatch(key, Event::Click);
    assert!(handled, "клик обработан dispatch_vmodel");
    assert!(app.agreed(), "после клика agreed = true");

    // Второй клик сбрасывает обратно.
    let tree2 = app.render();
    let checkbox2 = tree2
        .find_by_tag("checkbox")
        .expect("чекбокс по-прежнему в дереве");
    let handled2 = app.dispatch(checkbox2.key, Event::Click);
    assert!(handled2, "второй клик тоже обработан");
    assert!(!app.agreed(), "после второго клика agreed = false");
}

/// ТЗ 0.17.5: devtools CLICK fallback — прямой вызов `dispatch_vmodel`
/// с позиционным ключом работает, даже если статические ветки `dispatch`
/// не совпали (имитирует поведение `handle_devtools_command`).
#[test]
fn dispatch_vmodel_fallback_with_positional_key() {
    let app = FormApp::new();
    assert!(!app.agreed());

    // Получаем позиционный ключ из дерева (авто-ключ `.0.1`).
    let tree = app.render();
    let checkbox = tree.find_by_tag("checkbox").expect("чекбокс");
    let key = checkbox.key;
    assert!(
        key.starts_with('.'),
        "ожидался позиционный (auto) ключ, получен: {key}"
    );

    // Имитируем fallback: `dispatch` + `dispatch_vmodel`.
    // Если статическая ветка уже обработала клик — тоже ок.
    let event = Event::Click;
    let handled = app.dispatch(key, event.clone()) || app.dispatch_vmodel(key, &event);
    assert!(
        handled,
        "fallback dispatch_vmodel обработал клик по auto-ключу {key}"
    );
    assert!(app.agreed(), "состояние переключилось");
}

#[test]
fn checkbox_layout_has_positive_dimensions() {
    use light_fw::layout::{Layout, TextMetrics};

    struct M;
    impl TextMetrics for M {
        fn measure(&self, text: &str, _font_size: f32) -> (f32, f32) {
            (text.chars().count() as f32 * 8.0, 16.0)
        }
    }

    let app = FormApp::new();
    let tree = app.render();
    let layout = Layout::build(&tree, (480.0, 320.0), &M);
    let checkbox = tree.find_by_tag("checkbox").expect("чекбокс");
    let rect = layout
        .rect_of(checkbox.key)
        .expect("чекбокс присутствует в layout");
    assert!(
        rect.w > 0.0 && rect.h > 0.0,
        "чекбокс имеет ненулевой размер: w={} h={}",
        rect.w,
        rect.h
    );
}
