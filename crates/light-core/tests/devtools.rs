//! Интеграционные тесты модуля `devtools`: JSON-снимок VDOM и реактивных
//! свойств компонента.

#![cfg(feature = "devtools")]

extern crate light_fw_core as light;

use light::prelude::*;

/// Компонент из `tests/templates/counter.html`.
#[template("tests/templates/counter.html")]
#[proxy]
struct CounterApp {
    label: String,
    count: i64,
}

impl CounterApp {
    fn increment(&self) {
        self.set_count(self.count() + 1);
    }

    fn reset(&self) {
        self.set_count(0);
    }

    /// Кнопка «+1» блокируется при достижении максимума.
    fn is_max(&self) -> bool {
        self.count() >= 2
    }
}

/// Снимок содержит все ожидаемые ключи.
#[test]
fn snapshot_has_expected_keys() {
    let app = CounterApp::new();
    let snap = light::capture_snapshot(0, &app);
    let v = serde_json::to_value(&snap).expect("snapshot serializes");
    assert!(v.get("window_token").is_some(), "window_token key present");
    assert!(v.get("vdom").is_some(), "vdom key present");
    assert!(v.get("state").is_some(), "state key present");
    assert!(v.get("layout").is_some(), "layout key present");
}

/// Токен окна по умолчанию — `0`.
#[test]
fn snapshot_default_window_token_is_zero() {
    let app = CounterApp::new();
    let snap = light::capture_snapshot(0, &app);
    assert_eq!(snap.window_token, 0);
}

/// Раскладка пока не включается: `None`.
#[test]
fn snapshot_layout_is_none() {
    let app = CounterApp::new();
    let snap = light::capture_snapshot(0, &app);
    assert!(snap.layout.is_none());
}

/// Реактивные свойства появляются в снимке со значениями геттеров.
#[test]
fn snapshot_state_contains_reactive_properties() {
    let app = CounterApp::new();
    let snap = light::capture_snapshot(0, &app);
    let state = snap.state.as_object().expect("state is object");
    let expected_label = app.label();
    let expected_count = app.count();
    let got_label = state
        .get("label")
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| panic!("label missing in state"));
    let got_count = state
        .get("count")
        .and_then(|v| v.as_i64())
        .unwrap_or_else(|| panic!("count missing in state"));
    assert_eq!(got_label, &expected_label);
    assert_eq!(got_count, expected_count);
}

/// После изменения свойства снимок отражает новое значение.
#[test]
fn snapshot_reflects_reactive_updates() {
    let app = CounterApp::new();
    let before = light::capture_snapshot(0, &app).state;
    app.set_count(app.count() + 1);
    let after = light::capture_snapshot(0, &app).state;
    let before_count = before
        .as_object()
        .and_then(|o| o.get("count"))
        .and_then(|v| v.as_i64())
        .expect("count before");
    let after_count = after
        .as_object()
        .and_then(|o| o.get("count"))
        .and_then(|v| v.as_i64())
        .expect("count after");
    assert_eq!(after_count, before_count + 1);
}

/// Собирает все значения `tag` из JSON-дерева рекурсивно.
fn collect_tags(node: &serde_json::Value) -> Vec<&str> {
    let mut tags = Vec::new();
    if let Some(tag) = node.get("tag").and_then(|t| t.as_str()) {
        tags.push(tag);
    }
    if let Some(children) = node.get("children").and_then(|c| c.as_array()) {
        for child in children {
            tags.extend(collect_tags(child));
        }
    }
    tags
}

/// Находит первый узел с заданным `tag` рекурсивно.
fn find_tag<'a>(node: &'a serde_json::Value, tag: &'a str) -> Option<&'a serde_json::Value> {
    if node.get("tag").and_then(|t| t.as_str()) == Some(tag) {
        return Some(node);
    }
    if let Some(children) = node.get("children").and_then(|c| c.as_array()) {
        for child in children {
            if let Some(found) = find_tag(child, tag) {
                return Some(found);
            }
        }
    }
    None
}

/// VDOM содержит теги из шаблона (`div`, `button`, `text`).
#[test]
fn snapshot_vdom_contains_tags_from_template() {
    let app = CounterApp::new();
    let snap = light::capture_snapshot(0, &app);
    let tags = collect_tags(&snap.vdom);
    assert!(tags.contains(&"div"), "div found, got: {tags:?}");
    assert!(tags.contains(&"button"), "button found, got: {tags:?}");
    assert!(tags.contains(&"text"), "text found, got: {tags:?}");
}

/// Атрибуты сохраняются парами `[имя, значение]`.
#[test]
fn snapshot_vdom_preserves_attrs() {
    let app = CounterApp::new();
    let snap = light::capture_snapshot(0, &app);
    // Ищем элемент `<text>` с атрибутом `placeholder="Подпись"`.
    let text_node = find_tag(&snap.vdom, "text").expect("text node exists");
    let attrs = text_node
        .get("attrs")
        .and_then(|a| a.as_array())
        .expect("attrs array");
    let has_placeholder = attrs.iter().any(|pair| {
        pair.as_array()
            .map(|p| p.len() == 2 && p[0].as_str() == Some("placeholder"))
            .unwrap_or(false)
    });
    assert!(
        has_placeholder,
        "placeholder attr on <text>, attrs={attrs:?}"
    );
}

/// Компонент без реактивных свойств (не через `#[proxy]`) получает пустое
/// состояние.
#[test]
fn non_proxy_component_state_is_null() {
    struct Plain;
    impl Component for Plain {
        fn render(&self) -> VElement {
            VElement::container("", NodeKind::Column, vec![])
        }

        fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
            self
        }
    }
    let plain = Plain;
    let snap = light::capture_snapshot(0, &plain);
    assert!(snap.state.is_null(), "no reactive properties -> null");
}
