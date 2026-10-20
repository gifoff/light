//! Тесты построения дерева доступности.
#![cfg(feature = "a11y")]

extern crate light_fw_core as light;

use accesskit::{Invalid, Role};
use light::a11y::tree::{build_update, find_key_for_node_id, node_id_for_key};
use light::vdom::{ButtonType, NodeKind, VElement};

fn button(key: &'static str, label: &str) -> VElement {
    let child_key: &'static str = Box::leak(format!("{key}.t").into_boxed_str());
    VElement::container(
        key,
        NodeKind::Button {
            disabled: false,
            button_type: ButtonType::Submit,
        },
        vec![VElement::leaf(
            child_key,
            NodeKind::Text {
                content: label.into(),
            },
        )],
    )
}

fn column(key: &'static str, children: Vec<VElement>) -> VElement {
    VElement::container(key, NodeKind::Column, children)
}

fn window(key: &'static str, children: Vec<VElement>) -> VElement {
    VElement::container(key, NodeKind::Window, children)
}

fn with_attr(mut el: VElement, name: &'static str, value: &str) -> VElement {
    el.attrs.push((name, value.to_owned()));
    el
}

fn node_label(update: &accesskit::TreeUpdate, root: &VElement, key: &str) -> Option<String> {
    update
        .nodes
        .iter()
        .find(|(id, _)| find_key_for_node_id(root, *id) == Some(key))
        .and_then(|(_, node)| node.label().map(str::to_owned))
}

#[test]
fn node_id_is_stable_across_calls() {
    let a = node_id_for_key("0");
    let b = node_id_for_key("0");
    assert_eq!(a, b);
}

#[test]
fn node_id_differs_for_different_keys() {
    assert_ne!(node_id_for_key("0"), node_id_for_key("1"));
    assert_ne!(node_id_for_key(""), node_id_for_key("0"));
}

#[test]
fn find_key_round_trip() {
    let tree = column("", vec![button("0", "OK")]);
    let id = node_id_for_key("0");
    assert_eq!(find_key_for_node_id(&tree, id), Some("0"));
}

#[test]
fn find_key_for_root() {
    let tree = column("", vec![button("0", "OK")]);
    let id = node_id_for_key("");
    assert_eq!(find_key_for_node_id(&tree, id), Some(""));
}

#[test]
fn find_key_returns_none_for_unknown_id() {
    let tree = column("", vec![button("0", "OK")]);
    let stray = node_id_for_key("не-существует");
    assert_eq!(find_key_for_node_id(&tree, stray), None);
}

#[test]
fn build_update_contains_expected_roles() {
    let tree = column("", vec![button("0", "OK"), button("1", "Cancel")]);
    let update = build_update(&tree, None);

    let roles: Vec<Role> = update.nodes.iter().map(|(_, n)| n.role()).collect();
    assert!(roles.contains(&Role::Button), "roles = {roles:?}");
    assert!(roles.contains(&Role::GenericContainer), "roles = {roles:?}");

    let button_count = roles.iter().filter(|r| **r == Role::Button).count();
    assert_eq!(button_count, 2, "roles = {roles:?}");
}

#[test]
fn build_update_non_empty_for_single_button() {
    let tree = column("", vec![button("0", "OK")]);
    let update = build_update(&tree, None);
    assert!(!update.nodes.is_empty());
}

#[test]
fn focus_defaults_to_first_tabbable() {
    let tree = window(
        "",
        vec![
            column("0", vec![]),
            button("1", "OK"),
            button("2", "Cancel"),
        ],
    );
    let update = build_update(&tree, None);
    let key = find_key_for_node_id(&tree, update.focus).unwrap();
    assert_eq!(key, "1");
}

#[test]
fn focus_falls_back_to_root_when_nothing_tabbable() {
    let tree = window("", vec![column("0", vec![])]);
    let update = build_update(&tree, None);
    let key = find_key_for_node_id(&tree, update.focus).unwrap();
    assert_eq!(key, "");
}

#[test]
fn explicit_focus_overrides_default() {
    let tree = window("", vec![button("0", "A"), button("1", "B")]);
    let update = build_update(&tree, Some("1"));
    let key = find_key_for_node_id(&tree, update.focus).unwrap();
    assert_eq!(key, "1");
}

#[test]
fn explicit_focus_ignores_unknown_key() {
    let tree = window("", vec![button("0", "A")]);
    let update = build_update(&tree, Some("не-существует"));
    let key = find_key_for_node_id(&tree, update.focus).unwrap();
    assert_eq!(key, "0");
}

#[test]
fn container_has_no_derived_label() {
    let tree = window(
        "",
        vec![column(
            "0",
            vec![
                VElement::leaf(
                    "0.0",
                    NodeKind::Text {
                        content: "Привет".into(),
                    },
                ),
                VElement::leaf(
                    "0.1",
                    NodeKind::Text {
                        content: "Мир".into(),
                    },
                ),
            ],
        )],
    );
    let update = build_update(&tree, None);
    assert_eq!(node_label(&update, &tree, "0"), None);
}

#[test]
fn button_keeps_derived_label() {
    let tree = window("", vec![button("0", "Отправить")]);
    let update = build_update(&tree, None);
    assert_eq!(
        node_label(&update, &tree, "0").as_deref(),
        Some("Отправить")
    );
}

#[test]
fn container_with_aria_label_has_label() {
    let tree = window(
        "",
        vec![with_attr(column("0", vec![]), "aria-label", "Форма")],
    );
    let update = build_update(&tree, None);
    assert_eq!(node_label(&update, &tree, "0").as_deref(), Some("Форма"));
}

fn node_for<'a>(
    update: &'a accesskit::TreeUpdate,
    root: &VElement,
    key: &str,
) -> &'a accesskit::Node {
    update
        .nodes
        .iter()
        .find(|(id, _)| find_key_for_node_id(root, *id) == Some(key))
        .map(|(_, node)| node)
        .unwrap_or_else(|| panic!("узел `{key}` не найден"))
}

#[test]
fn required_and_invalid_attrs_reach_accesskit() {
    let field = with_attr(
        with_attr(button("0", "Поле"), "required", ""),
        "invalid",
        "true",
    );
    let tree = window("", vec![field]);
    let update = build_update(&tree, None);
    let node = node_for(&update, &tree, "0");
    assert!(node.is_required());
    assert_eq!(node.invalid(), Some(Invalid::True));
}

#[test]
fn aria_required_and_aria_invalid_variants() {
    let field = with_attr(
        with_attr(button("0", "Поле"), "aria-required", "true"),
        "aria-invalid",
        "true",
    );
    let tree = window("", vec![field]);
    let update = build_update(&tree, None);
    let node = node_for(&update, &tree, "0");
    assert!(node.is_required());
    assert_eq!(node.invalid(), Some(Invalid::True));
}

#[test]
fn aria_description_reaches_accesskit() {
    let tree = window(
        "",
        vec![with_attr(
            button("0", "Поле"),
            "aria-description",
            "Укажите имя",
        )],
    );
    let update = build_update(&tree, None);
    assert_eq!(
        node_for(&update, &tree, "0").description(),
        Some("Укажите имя")
    );
}

#[test]
fn build_update_respects_positive_tabindex_for_default_focus() {
    let tree = window(
        "",
        vec![
            button("0", "First in DOM"),
            with_attr(button("1", "Priority"), "tabindex", "1"),
        ],
    );
    let update = build_update(&tree, None);
    let key = find_key_for_node_id(&tree, update.focus).unwrap();
    assert_eq!(key, "1");
}

#[test]
fn build_update_falls_back_to_dom_order_without_tabindex() {
    let tree = window("", vec![button("0", "A"), button("1", "B")]);
    let update = build_update(&tree, None);
    let key = find_key_for_node_id(&tree, update.focus).unwrap();
    assert_eq!(key, "0");
}
