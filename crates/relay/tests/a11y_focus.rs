//! Тесты фокуса и порядка обхода клавишей Tab.
#![cfg(feature = "a11y")]

use relay::a11y::focus::{
    first_tabbable, focus_order, is_disabled, is_focusable, is_hidden, is_tabbable, next_focus,
    tabindex,
};
use relay::vdom::{ButtonType, NodeKind, Resize, VElement};

// ---------- Хелперы ----------

fn button(key: &'static str) -> VElement {
    VElement::leaf(
        key,
        NodeKind::Button {
            label: "OK".into(),
            disabled: false,
            button_type: ButtonType::Submit,
        },
    )
}

fn disabled_button(key: &'static str) -> VElement {
    VElement::leaf(
        key,
        NodeKind::Button {
            label: "OK".into(),
            disabled: true,
            button_type: ButtonType::Submit,
        },
    )
}

fn text_field(key: &'static str) -> VElement {
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

fn column(key: &'static str, children: Vec<VElement>) -> VElement {
    VElement::container(key, NodeKind::Column, children)
}

fn with_attr(mut el: VElement, name: &'static str, value: &str) -> VElement {
    el.attrs.push((name, value.to_owned()));
    el
}

fn hidden(mut el: VElement) -> VElement {
    el.style.visible = false;
    el
}

fn keys(order: &[&VElement]) -> Vec<&'static str> {
    order.iter().map(|e| e.key).collect()
}

// ---------- tabindex ----------

#[test]
fn tabindex_absent_returns_none() {
    assert_eq!(tabindex(&button("0")), None);
}

#[test]
fn tabindex_parses_numbers() {
    assert_eq!(tabindex(&with_attr(button("0"), "tabindex", "0")), Some(0));
    assert_eq!(tabindex(&with_attr(button("0"), "tabindex", "3")), Some(3));
    assert_eq!(
        tabindex(&with_attr(button("0"), "tabindex", "-1")),
        Some(-1)
    );
}

#[test]
fn tabindex_garbage_returns_none() {
    assert_eq!(tabindex(&with_attr(button("0"), "tabindex", "abc")), None);
    assert_eq!(tabindex(&with_attr(button("0"), "tabindex", "")), None);
}

// ---------- is_disabled ----------

#[test]
fn plain_button_is_not_disabled() {
    assert!(!is_disabled(&button("0")));
}

#[test]
fn aria_disabled_true_marks_disabled() {
    assert!(is_disabled(&with_attr(
        button("0"),
        "aria-disabled",
        "true"
    )));
}

#[test]
fn aria_disabled_false_does_not_disable() {
    assert!(!is_disabled(&with_attr(
        button("0"),
        "aria-disabled",
        "false"
    )));
}

#[test]
fn node_kind_disabled_marks_disabled() {
    assert!(is_disabled(&disabled_button("0")));
}

// ---------- is_hidden ----------

#[test]
fn visible_by_default() {
    assert!(!is_hidden(&button("0")));
}

#[test]
fn hidden_when_style_invisible() {
    assert!(is_hidden(&hidden(button("0"))));
}

// ---------- is_focusable ----------

#[test]
fn button_is_focusable() {
    assert!(is_focusable(&button("0")));
}

#[test]
fn text_field_is_focusable() {
    assert!(is_focusable(&text_field("0")));
}

#[test]
fn div_without_tabindex_is_not_focusable() {
    assert!(!is_focusable(&column("0", vec![])));
}

#[test]
fn div_with_tabindex_is_focusable() {
    assert!(is_focusable(&with_attr(
        column("0", vec![]),
        "tabindex",
        "0"
    )));
}

#[test]
fn hidden_element_is_not_focusable() {
    assert!(!is_focusable(&hidden(button("0"))));
}

#[test]
fn disabled_element_is_not_focusable() {
    assert!(!is_focusable(&disabled_button("0")));
}

// ---------- is_tabbable ----------

#[test]
fn button_is_tabbable() {
    assert!(is_tabbable(&button("0")));
}

#[test]
fn negative_tabindex_is_focusable_but_not_tabbable() {
    let el = with_attr(button("0"), "tabindex", "-1");
    assert!(is_focusable(&el));
    assert!(!is_tabbable(&el));
}

#[test]
fn div_with_tabindex_is_tabbable() {
    assert!(is_tabbable(&with_attr(
        column("0", vec![]),
        "tabindex",
        "0"
    )));
}

#[test]
fn div_without_tabindex_is_not_tabbable() {
    assert!(!is_tabbable(&column("0", vec![])));
}

// ---------- focus_order ----------

#[test]
fn focus_order_follows_dom_for_same_tabindex() {
    let tree = column("", vec![button("0"), button("1"), button("2")]);
    assert_eq!(keys(&focus_order(&tree)), vec!["0", "1", "2"]);
}

#[test]
fn positive_tabindex_comes_first() {
    let tree = column(
        "",
        vec![
            button("0"),
            with_attr(button("1"), "tabindex", "1"),
            button("2"),
        ],
    );
    assert_eq!(keys(&focus_order(&tree)), vec!["1", "0", "2"]);
}

#[test]
fn negative_tabindex_is_excluded_from_order() {
    let tree = column(
        "",
        vec![
            button("0"),
            with_attr(button("1"), "tabindex", "-1"),
            button("2"),
        ],
    );
    assert_eq!(keys(&focus_order(&tree)), vec!["0", "2"]);
}

#[test]
fn hidden_and_disabled_are_excluded_from_order() {
    let tree = column(
        "",
        vec![
            button("0"),
            hidden(button("1")),
            disabled_button("2"),
            button("3"),
        ],
    );
    assert_eq!(keys(&focus_order(&tree)), vec!["0", "3"]);
}

#[test]
fn nested_children_are_traversed_in_order() {
    let tree = column(
        "",
        vec![
            button("0"),
            column("1", vec![button("1.0"), button("1.1")]),
            button("2"),
        ],
    );
    // Порядок — обход в глубину, слева направо
    assert_eq!(keys(&focus_order(&tree)), vec!["0", "1.0", "1.1", "2"]);
}

// ---------- first_tabbable / next_focus ----------

#[test]
fn first_tabbable_returns_first_in_order() {
    let tree = column("", vec![column("0", vec![]), button("1"), button("2")]);
    assert_eq!(first_tabbable(&tree).map(|e| e.key), Some("1"));
}

#[test]
fn first_tabbable_returns_none_when_empty() {
    let tree = column("", vec![]);
    assert!(first_tabbable(&tree).is_none());
}

#[test]
fn next_focus_moves_forward() {
    let tree = column("", vec![button("0"), button("1"), button("2")]);
    assert_eq!(next_focus(&tree, "0", true).map(|e| e.key), Some("1"));
    assert_eq!(next_focus(&tree, "1", true).map(|e| e.key), Some("2"));
}

#[test]
fn next_focus_moves_backward() {
    let tree = column("", vec![button("0"), button("1"), button("2")]);
    assert_eq!(next_focus(&tree, "2", false).map(|e| e.key), Some("1"));
    assert_eq!(next_focus(&tree, "1", false).map(|e| e.key), Some("0"));
}

#[test]
fn next_focus_does_not_panic_at_ends() {
    // Поведение на краях (wrap или stop) — деталь реализации,
    // здесь проверяем, что нет паники и результат — опция.
    let tree = column("", vec![button("0"), button("1")]);
    let _ = next_focus(&tree, "1", true);
    let _ = next_focus(&tree, "0", false);
}
