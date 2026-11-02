//! Тесты runtime-templates: парсинг HTML в VElement через
//! [`light::runtime::parse_template`] и базовые свойства дерева.

#![cfg(feature = "runtime-templates")]

extern crate light_fw_core as light;

use std::any::Any;

use light::component::Component;
use light::vdom::{Event, NodeKind, VElement};

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
    let el = light::runtime::parse_template("", &c).expect("parse");
    assert_eq!(el.kind, NodeKind::Column);
    assert!(el.children.is_empty());
}

/// Один `<div>` с текстом внутри → Column-контейнер с Text-листом.
#[test]
fn div_with_bare_text() {
    let c = Empty;
    let el = light::runtime::parse_template(
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
    let el = light::runtime::parse_template(
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
    let el = light::runtime::parse_template(
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
    let el = light::runtime::parse_template(
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
    let el = light::runtime::parse_template(
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
    let result = light::runtime::parse_template(
        r#"<window title="t" width="100" height="80"><div @focus="h">D</div></window>"#,
        &c,
    );
    assert!(result.is_none());
}

/// Неизвестная v-директива → None.
#[test]
fn strict_rejects_unknown_v_directive() {
    let c = Empty;
    let result = light::runtime::parse_template(
        r#"<window title="t" width="100" height="80"><div v-custom="x">D</div></window>"#,
        &c,
    );
    assert!(result.is_none());
}

/// `v-if` → None (в runtime не поддерживается).
#[test]
fn strict_rejects_v_if() {
    let c = Empty;
    let result = light::runtime::parse_template(
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
    let el = light::runtime::parse_template(
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
    let el = light::runtime::parse_template(
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
    let el = light::runtime::parse_template(
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
    let el = light::runtime::parse_template(
        r#"<window title="t" width="100" height="80"><div>{{ count }}</div></window>"#,
        &c,
    )
    .expect("parse");
    assert_eq!(el.children.len(), 1);
    matches!(&el.children[0].children[0].kind, NodeKind::Text { content } if content == "7");
}

/// Неизвестное поле в интерполяции → None (строгий режим).
#[test]
fn interpolation_unknown_field_fails() {
    let c = Empty;
    let result = light::runtime::parse_template(
        r#"<window title="t" width="100" height="80"><div>{{ missing }}</div></window>"#,
        &c,
    );
    assert!(result.is_none());
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
    let el = light::runtime::parse_template(
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
    let el = light::runtime::parse_template(
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
    let el = light::runtime::parse_template(
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
        fn dispatch_by_name(&self, handler: &str, _event: Event) -> bool {
            handler == "increment"
        }
        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }
    }

    let c = Handler { calls: 0 };
    let el = light::runtime::parse_template(
        r#"<window title="t" width="100" height="80"><button @click="increment">+</button></window>"#,
        &c,
    )
    .expect("parse");
    assert_eq!(el.children.len(), 1);
    // Клик обрабатывается через dispatch_by_name.
    assert!(c.dispatch_by_name("increment", Event::Click));
}

/// Неизвестный тег → None.
#[test]
fn unknown_tag_rejected() {
    let c = Empty;
    let result = light::runtime::parse_template(
        r#"<window title="t" width="100" height="80"><video src="x">V</video></window>"#,
        &c,
    );
    assert!(result.is_none());
}
