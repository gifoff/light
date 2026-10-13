//! Тесты блокировки событий для отключённых узлов.
//!
//! Клик по `disabled`-элементу не должен доходить до его `@click` и не должен
//! всплывать к родителям. Единая проверка [`is_disabled`] используется и
//! оконным бэкендом (доставка событий, курсор), и клавиатурной активацией.

use relay::a11y::is_disabled;
use relay::layout::{Layout, Point, TextMetrics};
use relay::vdom::{ButtonType, NodeKind, SelectOption, VElement};

struct StubMetrics;

impl TextMetrics for StubMetrics {
    fn measure(&self, text: &str, _font_size: f32) -> (f32, f32) {
        (text.chars().count() as f32 * 8.0, 16.0)
    }
}

fn button(key: &'static str, disabled: bool) -> VElement {
    VElement::leaf(
        key,
        NodeKind::Button {
            label: "Отправить".to_owned(),
            disabled,
            button_type: ButtonType::Submit,
        },
    )
}

fn with_attr(mut node: VElement, name: &'static str, value: &str) -> VElement {
    node.attrs.push((name, value.to_owned()));
    node
}

#[test]
fn disabled_controls_are_reported_disabled() {
    let checkbox = VElement::leaf(
        "c",
        NodeKind::Checkbox {
            label: "Согласен".to_owned(),
            checked: false,
            disabled: true,
        },
    );
    let radio = VElement::leaf(
        "r",
        NodeKind::Radio {
            label: "А".to_owned(),
            value: "a".to_owned(),
            selected: false,
            disabled: true,
        },
    );
    let select = VElement::leaf(
        "s",
        NodeKind::Select {
            value: String::new(),
            options: Vec::<SelectOption>::new(),
            disabled: true,
        },
    );
    let field = VElement::leaf(
        "f",
        NodeKind::TextField {
            value: String::new(),
            placeholder: String::new(),
            disabled: true,
            multiline: false,
            resize: relay::vdom::Resize::None,
            masked: false,
        },
    );

    for element in [button("b", true), checkbox, radio, select, field] {
        assert!(is_disabled(&element), "узел {} отключён", element.key);
    }
}

#[test]
fn enabled_controls_are_not_disabled() {
    let field = VElement::leaf(
        "f",
        NodeKind::TextField {
            value: String::new(),
            placeholder: String::new(),
            disabled: false,
            multiline: false,
            resize: relay::vdom::Resize::None,
            masked: false,
        },
    );
    assert!(!is_disabled(&button("b", false)));
    assert!(!is_disabled(&field));
    assert!(!is_disabled(&VElement::leaf(
        "t",
        NodeKind::Text {
            content: "текст".to_owned(),
        },
    )));
}

#[test]
fn aria_disabled_marks_any_node() {
    let plain = with_attr(
        VElement::leaf(
            "d",
            NodeKind::Text {
                content: "подпись".to_owned(),
            },
        ),
        "aria-disabled",
        "true",
    );
    assert!(is_disabled(&plain), "aria-disabled блокирует любой узел");
}

#[test]
fn aria_disabled_false_does_not_override_flag() {
    let node = with_attr(button("b", true), "aria-disabled", "false");
    assert!(
        is_disabled(&node),
        "aria-disabled=false не снимает встроенный disabled"
    );
}

#[test]
fn disabled_node_stops_bubbling_before_parent() {
    let mut parent =
        VElement::container("form", NodeKind::Column, vec![button("gated", true)]).with_tag("div");
    parent.attrs.push(("role", "form".to_owned()));
    let layout = Layout::build(&parent, (200.0, 100.0), &StubMetrics);

    let rect = layout.rect_of("gated").expect("кнопка в раскладке");
    let point = Point::new(rect.x + rect.w / 2.0, rect.y + rect.h / 2.0);
    let stack = layout.hit_stack(point);

    assert_eq!(
        stack.first().map(|node| node.key),
        Some("gated"),
        "отключённый узел — первый в стеке попаданий"
    );
    assert!(
        stack.iter().any(|node| node.key == "form"),
        "родитель виден в стеке, но событие до него не дойдёт"
    );
    assert!(
        is_disabled(parent.get("gated").expect("узел")),
        "оконный бэкенд поглотит событие на этом узле"
    );
}
