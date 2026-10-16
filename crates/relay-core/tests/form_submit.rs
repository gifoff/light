//! Тесты поиска кнопки отправки формы (submit по `Enter`).

extern crate relay_core as relay;

use relay::validation::find_submit_button;
use relay::vdom::{ButtonType, NodeKind, Resize, VElement};

fn form(key: &'static str, children: Vec<VElement>) -> VElement {
    let mut node = VElement::container(key, NodeKind::Column, children);
    node.attrs.push(("role", "form".to_owned()));
    node
}

fn field(key: &'static str) -> VElement {
    VElement::leaf(
        key,
        NodeKind::TextField {
            value: String::new(),
            placeholder: String::new(),
            disabled: false,
            multiline: false,
            resize: Resize::None,
            masked: false,
        },
    )
}

fn button(key: &'static str, button_type: ButtonType, disabled: bool) -> VElement {
    VElement::leaf(
        key,
        NodeKind::Button {
            disabled,
            button_type,
        },
    )
}

#[test]
fn finds_first_submit_button_in_form() {
    let root = form(
        "form",
        vec![
            field("name"),
            button("cancel", ButtonType::Button, false),
            button("send", ButtonType::Submit, false),
        ],
    );
    assert_eq!(
        find_submit_button(&root, "name").map(|node| node.key),
        Some("send")
    );
}

#[test]
fn skips_reset_and_plain_buttons() {
    let root = form(
        "form",
        vec![
            field("name"),
            button("plain", ButtonType::Button, false),
            button("reset", ButtonType::Reset, false),
        ],
    );
    assert!(
        find_submit_button(&root, "name").is_none(),
        "кнопки type=button/reset форму не отправляют"
    );
}

#[test]
fn skips_disabled_submit_button() {
    let root = form(
        "form",
        vec![
            field("name"),
            button("gated", ButtonType::Submit, true),
            button("send", ButtonType::Submit, false),
        ],
    );
    assert_eq!(
        find_submit_button(&root, "name").map(|node| node.key),
        Some("send"),
        "отключённая кнопка пропускается"
    );
}

#[test]
fn returns_none_when_field_outside_form() {
    let root = VElement::container(
        "",
        NodeKind::Column,
        vec![field("name"), button("send", ButtonType::Submit, false)],
    );
    assert!(find_submit_button(&root, "name").is_none());
}

#[test]
fn returns_none_when_form_has_no_submit() {
    let root = form("form", vec![field("name")]);
    assert!(find_submit_button(&root, "name").is_none());
}

#[test]
fn picks_submit_of_nearest_form() {
    let inner = form(
        "inner",
        vec![
            field("name"),
            button("inner_send", ButtonType::Submit, false),
        ],
    );
    let outer = form(
        "outer",
        vec![button("outer_send", ButtonType::Submit, false), inner],
    );
    assert_eq!(
        find_submit_button(&outer, "name").map(|node| node.key),
        Some("inner_send"),
        "берётся кнопка ближайшей формы"
    );
}
