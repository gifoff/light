//! Тесты runtime-templates: парсинг HTML в VElement через
//! [`light_fw::runtime::parse_template`] и базовые свойства дерева.

#![cfg(feature = "runtime-templates")]

extern crate light_fw_core as light_fw;

use std::any::Any;

use light_fw::component::Component;
use light_fw::vdom::{Event, NodeKind, VElement};

/// Минимальный тестовый компонент без реактивных полей.
struct Empty;

impl Component for Empty {
    fn render(&self) -> VElement {
        VElement::container("", NodeKind::Column, Vec::new())
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

// ---------------------------------------------------------------------------
// Базовые случаи
// ---------------------------------------------------------------------------

/// Пустой шаблон → контейнер-колонка без детей.
#[test]
fn empty_html_gives_empty_column() {
    let c = Empty;
    let el = light_fw::runtime::parse_template("", &c).expect("parse");
    assert_eq!(el.kind, NodeKind::Column);
    assert!(el.children.is_empty());
}

/// Один `<div>` с текстом внутри → Column-контейнер с Text-листом.
#[test]
fn div_with_bare_text() {
    let c = Empty;
    let el = light_fw::runtime::parse_template(
        r#"<window title="t" width="100" height="80"><div>Hi</div></window>"#,
        &c,
    )
    .expect("parse");
    // Корневой контейнер (column от window) содержит один div, в нём bare text.
    assert_eq!(el.children.len(), 1);
    let inner = &el.children[0];
    assert_eq!(inner.kind, NodeKind::Column);
    assert_eq!(inner.children.len(), 1);
    matches!(&inner.children[0].kind, NodeKind::Text { content } if content == "Hi");
}

/// `<button>` → Button-контейнер; текст внутри → Text-лист.
#[test]
fn button_element() {
    let c = Empty;
    let el = light_fw::runtime::parse_template(
        r#"<window title="t" width="100" height="80"><button>Click</button></window>"#,
        &c,
    )
    .expect("parse");
    assert_eq!(el.children.len(), 1);
    let btn = &el.children[0];
    matches!(&btn.kind, NodeKind::Button { .. });
    assert_eq!(btn.key, ".0");
    assert_eq!(btn.children.len(), 1);
    matches!(&btn.children[0].kind, NodeKind::Text { content } if content == "Click");
}

/// `<h1>` → Column-контейнер с Text-листом внутри (heading font).
#[test]
fn h1_heading() {
    let c = Empty;
    let el = light_fw::runtime::parse_template(
        r#"<window title="t" width="100" height="80"><h1>Title</h1></window>"#,
        &c,
    )
    .expect("parse");
    assert_eq!(el.children.len(), 1);
    let h1 = &el.children[0];
    assert_eq!(h1.kind, NodeKind::Column);
    assert_eq!(h1.children.len(), 1);
    matches!(&h1.children[0].kind, NodeKind::Text { content } if content == "Title");
}

/// Несколько элементов → индексированные ключи: `.0`, `.1`.
#[test]
fn generated_key_from_index() {
    let c = Empty;
    let el = light_fw::runtime::parse_template(
        r#"<window title="t" width="100" height="80">
            <div>a</div>
            <div>b</div>
        </window>"#,
        &c,
    )
    .expect("parse");
    assert_eq!(el.children[0].key, ".0");
    assert_eq!(el.children[1].key, ".1");
}

/// Вложенный контейнер: ключ ребёнка — `{parent_key}.{index}`.
#[test]
fn nested_key_inheritance() {
    let c = Empty;
    let el = light_fw::runtime::parse_template(
        r#"<window title="t" width="100" height="80">
            <div id="outer">
                <div id="inner">x</div>
            </div>
        </window>"#,
        &c,
    )
    .expect("parse");
    let outer = &el.children[0];
    assert_eq!(outer.key, ".0");
    let inner = &outer.children[0];
    assert_eq!(inner.key, ".0.0");
}

// ---------------------------------------------------------------------------
// Строгий режим: недопустимые директивы
// ---------------------------------------------------------------------------

/// Неизвестный @-обработчик (не click) → None.
#[test]
fn strict_rejects_unknown_event_directive() {
    let c = Empty;
    let result = light_fw::runtime::parse_template(
        r#"<window title="t" width="100" height="80"><div @focus="h">D</div></window>"#,
        &c,
    );
    assert!(result.is_none());
}

/// Неизвестная v-директива → None.
#[test]
fn strict_rejects_unknown_v_directive() {
    let c = Empty;
    let result = light_fw::runtime::parse_template(
        r#"<window title="t" width="100" height="80"><div v-custom="x">D</div></window>"#,
        &c,
    );
    assert!(result.is_none());
}

/// `v-if` → None (в runtime не поддерживается).
#[test]
fn strict_rejects_v_if() {
    let c = Empty;
    let result = light_fw::runtime::parse_template(
        r#"<window title="t" width="100" height="80"><div v-if="x">D</div></window>"#,
        &c,
    );
    assert!(result.is_none());
}

// ---------------------------------------------------------------------------
// TextField (этементы `<text>` / `<password>`)
// ---------------------------------------------------------------------------

/// `<text>` без `v-model` → TextField с пустым значением.
#[test]
fn text_without_model_gives_empty_textfield() {
    let c = Empty;
    let el = light_fw::runtime::parse_template(
        r#"<window title="t" width="100" height="80"><text placeholder="Enter"></text></window>"#,
        &c,
    )
    .expect("parse");
    match &el.children[0].kind {
        NodeKind::TextField {
            value, placeholder, ..
        } => {
            assert_eq!(value, "");
            assert_eq!(placeholder, "Enter");
        }
        other => panic!("expected TextField, got {other:?}"),
    }
}

/// `<text v-model="name">` → TextField со значением из eval_field.
#[test]
fn text_with_model_reads_field() {
    struct HasName {
        #[allow(dead_code)]
        name: String,
    }
    impl Component for HasName {
        fn render(&self) -> VElement {
            VElement::container("", NodeKind::Column, Vec::new())
        }
        fn eval_field(&self, name: &str) -> Option<String> {
            if name == "name" {
                Some(self.name.clone())
            } else {
                None
            }
        }
        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }
    }

    let c = HasName {
        name: String::from("World"),
    };
    let el = light_fw::runtime::parse_template(
        r#"<window title="t" width="100" height="80"><text v-model="name"></text></window>"#,
        &c,
    )
    .expect("parse");
    match &el.children[0].kind {
        NodeKind::TextField { value, .. } => assert_eq!(value, "World"),
        other => panic!("expected TextField, got {other:?}"),
    }
}

/// `<password>` → TextField с masked=true.
#[test]
fn password_is_masked() {
    struct P {
        #[allow(dead_code)]
        pw: String,
    }
    impl Component for P {
        fn render(&self) -> VElement {
            VElement::container("", NodeKind::Column, Vec::new())
        }
        fn eval_field(&self, _name: &str) -> Option<String> {
            None
        }
        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }
    }

    let c = P { pw: String::new() };
    let el = light_fw::runtime::parse_template(
        r#"<window title="t" width="100" height="80"><password placeholder="pw"></password></window>"#,
        &c,
    )
    .expect("parse");
    match &el.children[0].kind {
        NodeKind::TextField {
            masked: true,
            placeholder,
            ..
        } => assert_eq!(placeholder, "pw"),
        other => panic!("expected masked TextField, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Интерполяция {{ field }}
// ---------------------------------------------------------------------------

/// Bare text с `{{ field }}` → значение из eval_field.
#[test]
fn interpolation_resolves_field() {
    struct Counter {
        #[allow(dead_code)]
        count: u32,
    }
    impl Component for Counter {
        fn render(&self) -> VElement {
            VElement::container("", NodeKind::Column, Vec::new())
        }
        fn eval_field(&self, name: &str) -> Option<String> {
            if name == "count" {
                Some(self.count.to_string())
            } else {
                None
            }
        }
        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }
    }

    let c = Counter { count: 7 };
    let el = light_fw::runtime::parse_template(
        r#"<window title="t" width="100" height="80"><div>{{ count }}</div></window>"#,
        &c,
    )
    .expect("parse");
    assert_eq!(el.children.len(), 1);
    matches!(&el.children[0].children[0].kind, NodeKind::Text { content } if content == "7");
}

/// Неизвестное поле в интерполяции → парсинг успешен, пустой текст отброшен.
#[test]
fn interpolation_unknown_field_gives_empty() {
    let c = Empty;
    let result = light_fw::runtime::parse_template(
        r#"<window title="t" width="100" height="80"><div>{{ missing }}</div></window>"#,
        &c,
    );
    let el = result.expect("unknown field should not fail the whole template");
    // Пустой текст не создаёт узла — div без детей.
    assert!(
        el.children[0].children.is_empty(),
        "empty interpolation text should produce no child nodes"
    );
}

// ---------------------------------------------------------------------------
// app.html (интеграционный тест: полный шаблон devtools-demo)
// ---------------------------------------------------------------------------

/// `parse_template` на реальном app.html devtools-demo → Some.
///
/// Шаблоны с методами (`{{ status }}`), где `status` — метод, а не поле,
/// должны парситься успешно: неизвестные поля дают пустую строку.
#[test]
fn parse_app_html_returns_some() {
    struct DemoApp {
        #[allow(dead_code)]
        count: i64,
    }
    impl Component for DemoApp {
        fn render(&self) -> VElement {
            VElement::container("", NodeKind::Column, Vec::new())
        }
        fn eval_field(&self, name: &str) -> Option<String> {
            if name == "count" {
                Some(self.count.to_string())
            } else {
                None
            }
        }
        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }
    }
    let c = DemoApp { count: 0 };
    let html = include_str!("../../../examples/devtools-demo/templates/app.html");
    let el = light_fw::runtime::parse_template(html, &c).expect("app.html must parse at runtime");
    // Корневой window содержит titlebar и body div.
    assert!(
        el.children.len() >= 2,
        "expected titlebar + body, got {}",
        el.children.len()
    );
}

// ---------------------------------------------------------------------------
// Checkbox / Radio / Select (features elements-*)
// ---------------------------------------------------------------------------

/// `<checkbox v-model="checked">label</checkbox>` → NodeKind::Checkbox.
#[test]
#[cfg(feature = "elements-checkbox")]
fn checkbox_element() {
    struct HasChecked {
        #[allow(dead_code)]
        checked: bool,
    }
    impl Component for HasChecked {
        fn render(&self) -> VElement {
            VElement::container("", NodeKind::Column, Vec::new())
        }
        fn eval_field(&self, name: &str) -> Option<String> {
            if name == "checked" {
                Some("false".to_owned())
            } else {
                None
            }
        }
        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }
    }

    let c = HasChecked { checked: false };
    let el = light_fw::runtime::parse_template(
        r#"<window title="t" width="100" height="80"><checkbox v-model="checked">On</checkbox></window>"#,
        &c,
    )
    .expect("parse");
    matches!(&el.children[0].kind, NodeKind::Checkbox { .. });
}

/// `<radio v-model="sel" value="a">A</radio>` → NodeKind::Radio.
#[test]
#[cfg(feature = "elements-radio")]
fn radio_element() {
    struct HasSel {
        #[allow(dead_code)]
        sel: String,
    }
    impl Component for HasSel {
        fn render(&self) -> VElement {
            VElement::container("", NodeKind::Column, Vec::new())
        }
        fn eval_field(&self, name: &str) -> Option<String> {
            if name == "sel" {
                Some(self.sel.clone())
            } else {
                None
            }
        }
        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }
    }

    let c = HasSel {
        sel: String::from("a"),
    };
    let el = light_fw::runtime::parse_template(
        r#"<window title="t" width="100" height="80"><radio v-model="sel" value="a">A</radio></window>"#,
        &c,
    )
    .expect("parse");
    matches!(&el.children[0].kind, NodeKind::Radio { .. });
}

/// `<select v-model="opt">` с `<option>` → NodeKind::Select.
#[test]
#[cfg(feature = "elements-select")]
fn select_element() {
    struct HasOpt {
        #[allow(dead_code)]
        opt: String,
    }
    impl Component for HasOpt {
        fn render(&self) -> VElement {
            VElement::container("", NodeKind::Column, Vec::new())
        }
        fn eval_field(&self, name: &str) -> Option<String> {
            if name == "opt" {
                Some(self.opt.clone())
            } else {
                None
            }
        }
        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }
    }

    let c = HasOpt {
        opt: String::from("1"),
    };
    let el = light_fw::runtime::parse_template(
        r#"<window title="t" width="100" height="80">
            <select v-model="opt">
                <option value="1">One</option>
                <option value="2">Two</option>
            </select>
        </window>"#,
        &c,
    )
    .expect("parse");
    match &el.children[0].kind {
        NodeKind::Select { options, .. } => {
            assert_eq!(options.len(), 2);
            assert_eq!(options[0].label, "One");
            assert_eq!(options[1].label, "Two");
        }
        other => panic!("expected Select, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// @click (нуларгументные обработчики)
// ---------------------------------------------------------------------------

/// @click="increment" (простое имя) → парсинг успешен; handler хранится.
#[test]
fn simple_click_handler_accepted() {
    struct Handler {
        #[allow(dead_code)]
        calls: u32,
    }
    impl Component for Handler {
        fn render(&self) -> VElement {
            VElement::container("", NodeKind::Column, Vec::new())
        }
        // Макрос не подключён: `invoke_handler` перехватывает имя, не попавшее
        // в стандартные (close/minimize/…).
        fn invoke_handler(&self, handler: &str, _event: Event) -> bool {
            handler == "increment"
        }
        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }
    }

    let c = Handler { calls: 0 };
    let el = light_fw::runtime::parse_template(
        r#"<window title="t" width="100" height="80"><button @click="increment">+</button></window>"#,
        &c,
    )
    .expect("parse");
    assert_eq!(el.children.len(), 1);
    // Клик диспетчеризуется по имени обработчика через dispatch_by_name
    // (базовый хвост) → invoke_handler.
    assert!(c.dispatch_by_name("increment", Event::Click));
    // Чужое имя — нет.
    assert!(!c.dispatch_by_name("other", Event::Click));
}

/// Неизвестный тег → None.
#[test]
fn unknown_tag_rejected() {
    let c = Empty;
    let result = light_fw::runtime::parse_template(
        r#"<window title="t" width="100" height="80"><video src="x">V</video></window>"#,
        &c,
    );
    assert!(result.is_none());
}

// ---------------------------------------------------------------------------
// dispatch_by_name: фильтрация по типу события (регрессия A-fix7)
// ---------------------------------------------------------------------------

/// Тестовый компонент: не переопределяет `dispatch_by_name` (тестируем
/// базовую реализацию с Click-фильтром), перехватывает `invoke_handler`.
struct CountingHandler;

impl Component for CountingHandler {
    fn render(&self) -> VElement {
        VElement::container("", NodeKind::Column, Vec::new())
    }
    fn invoke_handler(&self, handler: &str, event: Event) -> bool {
        // Базовая реализация dispatch_by_name уже проверяет Click;
        // здесь только учитываем точное имя.
        handler == "increment" && matches!(event, Event::Click)
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// dispatch_by_name("increment", Mousemove) → false (обработчик НЕ вызван).
#[test]
fn dispatch_by_name_rejects_mousemove() {
    let c = CountingHandler;
    let result = c.dispatch_by_name("increment", Event::Mousemove { x: 5.0, y: 3.0 });
    assert!(
        !result,
        "non-Click events must not trigger @click handlers by name"
    );
}

/// dispatch_by_name("close", Mouseenter) → false (встроенный метод НЕ вызван).
#[test]
fn dispatch_by_name_rejects_mouseenter_on_builtin() {
    let c = CountingHandler;
    let result = c.dispatch_by_name("close", Event::Mouseenter);
    assert!(!result, "builtin 'close' must not fire on non-Click events");
}

/// dispatch_by_name("increment", Click) → true (обработчик вызван).
#[test]
fn dispatch_by_name_accepts_click() {
    let c = CountingHandler;
    let result = c.dispatch_by_name("increment", Event::Click);
    assert!(result, "Click events must trigger @click handlers by name");
}
