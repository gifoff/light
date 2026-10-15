//! Snapshot-тесты дерева доступности: фиксируют структуру `TreeUpdate`,
//! которую Relay отправляет в AccessKit, в человекочитаемом виде.
//!
//! Запуск:
//!   cargo test -p relay --features a11y --test a11y_snapshots
//!
//! Первый запуск сгенерирует эталоны и упадёт — это ожидаемо.
//! Примите их:
//!   cargo insta accept
//!
//! После этого любые изменения в дереве будут показаны в diff'е.

#![cfg(all(
    feature = "a11y",
    feature = "elements-checkbox",
    feature = "elements-radio",
    feature = "elements-select"
))]

use accesskit::TreeUpdate;
use relay::a11y::tree::{build_update, find_key_for_node_id};
use relay::vdom::{ButtonType, NodeKind, Resize, SelectOption, VElement};
use serde_json::json;

// ---------- Хелперы для сборки VElement ----------

fn button(key: &'static str, label: &str) -> VElement {
    button_with(key, label, false)
}

fn button_disabled(key: &'static str, label: &str) -> VElement {
    button_with(key, label, true)
}

fn button_with(key: &'static str, label: &str, disabled: bool) -> VElement {
    let child_key: &'static str = Box::leak(format!("{key}.t").into_boxed_str());
    VElement::container(
        key,
        NodeKind::Button {
            disabled,
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

fn text_field(key: &'static str, value: &str, placeholder: &str) -> VElement {
    VElement::leaf(
        key,
        NodeKind::TextField {
            value: value.into(),
            placeholder: placeholder.into(),
            disabled: false,
            multiline: false,
            resize: Resize::None,
            masked: false,
        },
    )
}

fn password_field(key: &'static str, value: &str) -> VElement {
    VElement::leaf(
        key,
        NodeKind::TextField {
            value: value.into(),
            placeholder: String::new(),
            disabled: false,
            multiline: false,
            resize: Resize::None,
            masked: true,
        },
    )
}

fn checkbox(key: &'static str, label: &str, checked: bool) -> VElement {
    VElement::leaf(
        key,
        NodeKind::Checkbox {
            label: label.into(),
            checked,
            disabled: false,
        },
    )
}

fn radio(key: &'static str, label: &str, value: &str, selected: bool) -> VElement {
    VElement::leaf(
        key,
        NodeKind::Radio {
            label: label.into(),
            value: value.into(),
            selected,
            disabled: false,
        },
    )
}

fn select(key: &'static str, value: &str, options: &[(&str, &str)]) -> VElement {
    VElement::leaf(
        key,
        NodeKind::Select {
            value: value.into(),
            options: options
                .iter()
                .map(|(l, v)| SelectOption {
                    label: (*l).into(),
                    value: (*v).into(),
                })
                .collect(),
            disabled: false,
        },
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

// ---------- Проекция TreeUpdate в читаемый JSON ----------

/// Превращает `TreeUpdate` в компактный список узлов, оставляя только то,
/// что важно для скринридера: роль, имя, значение, состояния, признак фокуса.
///
/// Мы не сериализуем сам `TreeUpdate` напрямую, чтобы снапшоты не зависели
/// от служебных полей accesskit (bounds, transform, toolkit-метаданные).
fn summarize(update: &TreeUpdate, root: &VElement) -> serde_json::Value {
    let nodes: Vec<_> = update
        .nodes
        .iter()
        .map(|(id, node)| {
            let key = find_key_for_node_id(root, *id).unwrap_or("?");
            json!({
                "key": key,
                "role": format!("{:?}", node.role()),
                "label": node.label(),
            })
        })
        .collect();

    let root_key = update
        .tree
        .as_ref()
        .map(|t| find_key_for_node_id(root, t.root).unwrap_or("?"))
        .unwrap_or("?");

    let focus_key = find_key_for_node_id(root, update.focus).unwrap_or("?");

    json!({
        "root": root_key,
        "focus": focus_key,
        "nodes": nodes,
    })
}

fn snap(name: &str, tree: &VElement) {
    let update = build_update(tree, None);
    insta::assert_json_snapshot!(name, summarize(&update, tree));
}

// ---------- Тесты ----------

#[test]
fn single_button() {
    let tree = window("", vec![button("0", "OK")]);
    snap("single_button", &tree);
}

#[test]
fn disabled_button() {
    let tree = window("", vec![button_disabled("0", "Сохранить")]);
    snap("disabled_button", &tree);
}

#[test]
fn text_field_with_placeholder() {
    let tree = window("", vec![text_field("0", "", "Введите имя")]);
    snap("text_field_with_placeholder", &tree);
}

#[test]
fn password_field_snapshot() {
    let tree = window("", vec![password_field("0", "secret")]);
    snap("password_field", &tree);
}

#[test]
fn checkbox_checked_and_unchecked() {
    let tree = window(
        "",
        vec![
            checkbox("0", "Согласен", true),
            checkbox("1", "Подписаться", false),
        ],
    );
    snap("checkbox_states", &tree);
}

#[test]
fn radio_group() {
    let tree = window(
        "",
        vec![
            radio("0", "Бесплатный", "free", false),
            radio("1", "Pro", "pro", true),
            radio("2", "Команда", "team", false),
        ],
    );
    snap("radio_group", &tree);
}

#[test]
fn select_with_options() {
    let tree = window(
        "",
        vec![select(
            "0",
            "msk",
            &[("Москва", "msk"), ("Санкт-Петербург", "spb")],
        )],
    );
    snap("select_with_options", &tree);
}

#[test]
fn simple_form() {
    let tree = window(
        "",
        vec![column(
            "0",
            vec![
                text_field("0.0", "", "Имя"),
                text_field("0.1", "", "Email"),
                checkbox("0.2", "Согласен с условиями", false),
                button("0.3", "Отправить"),
            ],
        )],
    );
    snap("simple_form", &tree);
}

#[test]
fn aria_label_overrides_placeholder() {
    // aria-label должен победить placeholder в качестве доступного имени.
    let el = with_attr(
        text_field("0", "", "Введите email"),
        "aria-label",
        "Электронная почта",
    );
    let tree = window("", vec![el]);
    snap("aria_label_wins", &tree);
}

#[test]
fn heading_levels_1_to_5() {
    let mut children = Vec::new();
    for (i, tag) in ["h1", "h2", "h3", "h4", "h5"].iter().enumerate() {
        let key: &'static str = Box::leak(format!("head{i}").into_boxed_str());
        let mut el = VElement::leaf(
            key,
            NodeKind::Text {
                content: tag.to_string(),
            },
        );
        el.tag = tag;
        children.push(el);
    }
    let tree = window("", vec![column("col", children)]);
    snap("headings", &tree);
}

#[test]
fn placeholder_becomes_label_when_no_aria() {
    let tree = window("", vec![text_field("0", "", "Введите имя")]);
    let update = build_update(&tree, None);
    let field = update
        .nodes
        .iter()
        .find(|(id, _)| find_key_for_node_id(&tree, *id) == Some("0"))
        .unwrap();
    assert_eq!(field.1.label(), Some("Введите имя"));
}

#[test]
fn aria_label_beats_placeholder() {
    let el = with_attr(
        text_field("0", "", "Введите email"),
        "aria-label",
        "Электронная почта",
    );
    let tree = window("", vec![el]);
    let update = build_update(&tree, None);
    let field = update
        .nodes
        .iter()
        .find(|(id, _)| find_key_for_node_id(&tree, *id) == Some("0"))
        .unwrap();
    assert_eq!(field.1.label(), Some("Электронная почта"));
}
