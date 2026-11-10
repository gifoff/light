//! Интеграционные тесты модуля `devtools`: JSON-снимок VDOM и реактивных
//! свойств компонента.

#![cfg(feature = "devtools")]

extern crate light_fw_core as light_fw;

use light_fw::prelude::*;

/// Компонент из `tests/templates/counter.html`.
#[template("tests/templates/counter.html")]
#[proxy]
struct CounterApp {
    label: String,
    count: i64,
}

impl CounterApp {
    fn increment(&self) -> bool {
        self.set_count(self.count() + 1);
        true
    }

    fn reset(&self) -> bool {
        self.set_count(0);
        true
    }

    /// Кнопка «+1» блокируется при достижении максимума.
    fn is_max(&self) -> bool {
        self.count() >= 2
    }
}

/// Компонент с вычисляемым значением (ТЗ 0.14.3-fix6): `status` — метод в
/// `impl`, а не поле структуры. Методы никогда не попадают ни в
/// `inspect_state`, ни в `inject_state` (состояние — это только поля) и
/// пересчитываются при каждом рендере; `eval_field` видит его, потому что
/// шаблон содержит `{{ status }}` (и `{{ count }}`).
#[template("tests/templates/computed.html")]
#[proxy]
struct TransientApp {
    count: i64,
}

impl TransientApp {
    /// Вычисляется из `count`; не хранится в state.
    fn status(&self) -> String {
        if self.count() == 0 {
            "Кликните «Инкремент»".to_owned()
        } else {
            format!("Тест: {}", self.count())
        }
    }
}

/// Снимок содержит все ожидаемые ключи.
#[test]
fn snapshot_has_expected_keys() {
    let app = CounterApp::new();
    let snap = light_fw::capture_snapshot(0, &app);
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
    let snap = light_fw::capture_snapshot(0, &app);
    assert_eq!(snap.window_token, 0);
}

/// Раскладка пока не включается: `None`.
#[test]
fn snapshot_layout_is_none() {
    let app = CounterApp::new();
    let snap = light_fw::capture_snapshot(0, &app);
    assert!(snap.layout.is_none());
}

/// Реактивные свойства появляются в снимке со значениями геттеров.
#[test]
fn snapshot_state_contains_reactive_properties() {
    let app = CounterApp::new();
    let snap = light_fw::capture_snapshot(0, &app);
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
    let before = light_fw::capture_snapshot(0, &app).state;
    app.set_count(app.count() + 1);
    let after = light_fw::capture_snapshot(0, &app).state;
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
    // После одного инкремента счётчик ровно `1` (а не `null`): снимок
    // несёт реальный JSON-значимый реактивный статус.
    assert_eq!(before_count, 0);
    assert_eq!(after_count, 1);
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
    let snap = light_fw::capture_snapshot(0, &app);
    let tags = collect_tags(&snap.vdom);
    assert!(tags.contains(&"div"), "div found, got: {tags:?}");
    assert!(tags.contains(&"button"), "button found, got: {tags:?}");
    assert!(tags.contains(&"text"), "text found, got: {tags:?}");
}

/// Атрибуты сохраняются парами `[имя, значение]`.
#[test]
fn snapshot_vdom_preserves_attrs() {
    let app = CounterApp::new();
    let snap = light_fw::capture_snapshot(0, &app);
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
    let snap = light_fw::capture_snapshot(0, &plain);
    assert!(snap.state.is_null(), "no reactive properties -> null");
}

/// Полный сброс состояния: `inject_state` перезаписывает оба реактивных поля,
/// а геттеры возвращают новые значения.
#[test]
fn inject_state_updates_all_fields() {
    let mut app = CounterApp::new();
    // Начальные значения — из шаблона (label) и по умолчанию (count = 0).
    let original_label = app.label().to_owned();
    let state = serde_json::json!({
        "label": "Счётчик после инъекции",
        "count": 42
    });
    app.inject_state(&state);
    assert_eq!(app.label(), "Счётчик после инъекции");
    assert_eq!(app.count(), 42);
    // Убедимся, что действительно изменилось (label из шаблона — не та же строка).
    assert_ne!(original_label, "Счётчик после инъекции");
}

/// Частичная инъекция: поле, отсутствующее в состоянии, сохраняет своё значение.
#[test]
fn inject_state_partial_leaves_missing_fields_untouched() {
    let mut app = CounterApp::new();
    let original_label = app.label().to_owned();
    let state = serde_json::json!({
        "count": 7
    });
    app.inject_state(&state);
    assert_eq!(app.count(), 7);
    // `label` отсутствует в состоянии — не должен измениться.
    assert_eq!(app.label(), original_label);
}

/// Поле с несовместимым типом игнорируется: значение не применяется, другое
/// поле — применяется.
#[test]
fn inject_state_wrong_type_is_ignored() {
    let mut app = CounterApp::new();
    let state = serde_json::json!({
        "label": "текст",
        "count": "не число"
    });
    app.inject_state(&state);
    // `count` — строка вместо i64: отброшено, остаётся начальное (0).
    assert_eq!(app.count(), 0);
    // `label` — корректный String: применён.
    assert_eq!(app.label(), "текст");
}

/// Пустое состояние ничего не меняет: геттеры возвращают начальные значения.
#[test]
fn inject_state_empty_object_is_noop() {
    let mut app = CounterApp::new();
    let label_before = app.label().to_owned();
    let count_before = app.count();
    app.inject_state(&serde_json::json!({}));
    assert_eq!(app.label(), label_before);
    assert_eq!(app.count(), count_before);
}

/// ТЗ 0.14.3-fix6: вычисляемое значение (`status` — метод, а не поле) не
/// попадает в снимок состояния — в JSON есть только `count`.
#[test]
fn method_not_in_inspect_state() {
    let app = TransientApp::new();
    let snap = light_fw::capture_snapshot(0, &app);
    let state = &snap.state;
    let obj = state
        .as_object()
        .unwrap_or_else(|| panic!("state должен быть объектом, получено: {state:?}"));
    assert!(obj.contains_key("count"), "count присутствует в state");
    assert!(
        !obj.contains_key("status"),
        "метод status не является полем и отсутствует в state: {state:?}"
    );
}

/// ТЗ 0.14.3-fix6: `inject_state` восстанавливает `count`, но ключ `status`
/// игнорируется (метод, а не поле): вычисленное значение всегда берётся из
/// кода на основе `count`, даже если в state лежит «застаревший» текст.
#[test]
fn method_not_restored_by_inject_state() {
    let mut app = TransientApp::new();
    // count = 0 → статус по умолчанию из кода.
    let default_status = app.status();
    assert_eq!(default_status.as_str(), "Кликните «Инкремент»");
    let state = serde_json::json!({
        "count": 7,
        "status": "ЗАСТАРЕВШИЙ ТЕКСТ"
    });
    app.inject_state(&state);
    assert_eq!(app.count(), 7, "count восстановлен из state");
    // Статус пересчитан из нового count, а не восстановлен из state.
    assert_eq!(app.status(), format!("Тест: {}", app.count()));
    assert_ne!(app.status(), "ЗАСТАРЕВШИЙ ТЕКСТ");
}

/// ТЗ 0.14.3-fix6: `eval_field` видит и ПОЛЯ, и МЕТОДЫ, используемые в
/// шаблоне. Для `{{ status }}` (метод) возвращает свежее вычисленное
/// значение; несуществующий идентификатор — `None`.
#[test]
fn eval_field_returns_method_and_field() {
    let app = TransientApp::new();
    // count = 0 → `{{ status }}` даёт дефолтный текст.
    let got_status = app.eval_field("status");
    assert_eq!(
        got_status.as_deref(),
        Some(app.status().as_str()),
        "eval_field видит метод status, потому что он используется в шаблоне"
    );
    // count — примитивное поле: тоже видно.
    let got_count = app.eval_field("count");
    assert_eq!(got_count.as_deref(), Some("0"));
    // Изменим state → метод пересчитывается (computed), a не читает старое.
    app.set_count(5);
    assert_eq!(app.eval_field("status").as_deref(), Some("Тест: 5"));
    assert_eq!(app.eval_field("count").as_deref(), Some("5"));
    // Контроль: не-существующий идентификатор даёт None.
    assert_eq!(app.eval_field("не_существует"), None);
}
