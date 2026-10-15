//! Тесты сравнения виртуальных деревьев: точечные обновления.

use relay::vdom::{NodeKind, Op, Patch, SelectOption, VElement, diff};

fn text(key: &'static str, content: &str) -> VElement {
    VElement::leaf(
        key,
        NodeKind::Text {
            content: content.to_owned(),
        },
    )
}

fn button(key: &'static str, label: &str, disabled: bool) -> VElement {
    VElement::container(
        key,
        NodeKind::Button {
            disabled,
            button_type: relay::vdom::ButtonType::Submit,
        },
        vec![VElement::leaf(
            "t",
            NodeKind::Text {
                content: label.to_owned(),
            },
        )],
    )
}

fn option(label: &str, value: &str) -> SelectOption {
    SelectOption {
        label: label.to_owned(),
        value: value.to_owned(),
    }
}

fn field(key: &'static str, value: &str) -> VElement {
    VElement::leaf(
        key,
        NodeKind::TextField {
            value: value.to_owned(),
            placeholder: String::new(),
            disabled: false,
            multiline: false,
            resize: relay::vdom::Resize::None,
            masked: false,
        },
    )
}

fn column(key: &'static str, children: Vec<VElement>) -> VElement {
    VElement::container(key, NodeKind::Column, children)
}

#[test]
fn identical_trees_produce_no_ops() {
    let old = column("", vec![text("0", "Привет"), button("1", "Ок", false)]);
    let new = old.clone();
    let patch = diff(&old, &new);
    assert!(patch.is_empty(), "без изменений не должно быть операций");
    assert!(patch.changed.is_empty());
}

#[test]
fn single_text_change_touches_only_that_node() {
    let old = column(
        "",
        vec![
            text("0", "Счётчик: 0"),
            button("1", "+1", false),
            field("2", ""),
        ],
    );
    let new = column(
        "",
        vec![
            text("0", "Счётчик: 1"),
            button("1", "+1", false),
            field("2", ""),
        ],
    );
    let patch = diff(&old, &new);

    assert_eq!(
        patch.ops,
        vec![Op::SetContent {
            key: "0",
            content: "Счётчик: 1".to_owned(),
        }],
        "обновляется только изменившийся текстовый узел"
    );
    assert_eq!(patch.changed.iter().copied().collect::<Vec<_>>(), vec!["0"]);
}

#[test]
fn button_label_and_disabled_are_separate_ops() {
    let old = button("0", "Старо", false);
    let label_only = button("0", "Ново", false);
    let patch = diff(&old, &label_only);
    assert_eq!(
        patch.ops,
        vec![Op::SetContent {
            key: "t",
            content: "Ново".to_owned(),
        }],
        "подпись кнопки хранится в текстовом ребёнке"
    );

    let disabled_only = button("0", "Старо", true);
    let patch = diff(&old, &disabled_only);
    assert_eq!(
        patch.ops,
        vec![Op::SetDisabled {
            key: "0",
            disabled: true,
        }]
    );
}

#[test]
fn textfield_value_change_is_set_value_op() {
    let old = field("0", "А");
    let new = field("0", "Аб");
    let patch = diff(&old, &new);
    assert_eq!(
        patch.ops,
        vec![Op::SetValue {
            key: "0",
            value: "Аб".to_owned(),
        }]
    );
}

#[test]
fn v_if_removal_and_insertion() {
    let with_both = column("", vec![text("0", "Видно"), text("1", "Скрыто")]);
    let without_second = column("", vec![text("0", "Видно")]);

    let patch = diff(&with_both, &without_second);
    assert_eq!(patch.ops, vec![Op::Remove { key: "1" }]);

    let patch = diff(&without_second, &with_both);
    assert_eq!(patch.ops, vec![Op::Insert { key: "1" }]);
}

#[test]
fn kind_change_replaces_node() {
    let old = VElement::leaf("0", NodeKind::Column);
    let new = VElement::leaf("0", NodeKind::Row);
    let patch = diff(&old, &new);
    assert_eq!(patch.ops, vec![Op::Replace { key: "0" }]);
    assert!(patch.changed.contains("0"));
}

#[test]
fn nested_change_updates_only_deep_node() {
    let old = column(
        "",
        vec![column(
            "0",
            vec![text("0.0", "глубоко: 0"), text("0.1", "нетронуто")],
        )],
    );
    let new = column(
        "",
        vec![column(
            "0",
            vec![text("0.0", "глубоко: 1"), text("0.1", "нетронуто")],
        )],
    );
    let patch: Patch = diff(&old, &new);
    assert_eq!(patch.ops.len(), 1, "одна операция на глубине");
    assert_eq!(patch.ops[0].key(), "0.0");
}

#[test]
fn descendants_walk_depth_first() {
    let tree = column("", vec![text("0", "А"), button("1", "Б", false)]);
    let keys: Vec<&str> = tree
        .descendants()
        .into_iter()
        .map(|node| node.key)
        .collect();
    assert_eq!(keys, vec!["", "0", "1", "t"]);
}

#[test]
fn descendants_find_support_collection_chain() {
    let tree = column("", vec![text("0", "А"), button("1", "Б", false)]);
    let buttons = tree
        .descendants()
        .into_iter()
        .filter(|node| matches!(node.kind, NodeKind::Button { .. }))
        .count();
    assert_eq!(buttons, 1);
}

#[test]
fn finds_nodes_by_tag_attr_and_content() {
    let tree = VElement::container(
        "",
        NodeKind::Column,
        vec![
            VElement::leaf(
                "0",
                NodeKind::Text {
                    content: "Заголовок".to_owned(),
                },
            )
            .with_tag("h2"),
            VElement::container(
                "1",
                NodeKind::Button {
                    disabled: false,
                    button_type: relay::vdom::ButtonType::Submit,
                },
                vec![VElement::leaf(
                    "1.0",
                    NodeKind::Text {
                        content: "Сохранить".to_owned(),
                    },
                )],
            )
            .with_tag("button")
            .with_attr("class", "primary"),
        ],
    );

    assert_eq!(tree.find_by_tag("h2").map(|node| node.key), Some("0"));
    assert_eq!(tree.find_by_tag("button").map(|node| node.key), Some("1"));
    assert!(tree.find_by_tag("select").is_none(), "нет такого тега");

    assert_eq!(
        tree.find_by_attr("class", "primary").map(|node| node.key),
        Some("1")
    );
    assert!(tree.find_by_attr("class", "secondary").is_none());
    assert_eq!(
        tree.get("1").and_then(|node| node.attr("class")),
        Some("primary")
    );

    assert_eq!(
        tree.find_by_content("Сохранить").map(|node| node.key),
        Some("1.0"),
        "текст кнопки — отдельный текстовый узел"
    );
    assert_eq!(
        tree.find_containing("голов").map(|node| node.key),
        Some("0"),
        "поиск по части содержимого"
    );
    assert!(tree.find_containing("Заголовки").is_none());
}

#[test]
fn select_value_and_options_produce_specific_ops() {
    let before = VElement::leaf(
        "s",
        NodeKind::Select {
            value: "a".to_owned(),
            options: vec![option("A", "a")],
            disabled: false,
        },
    );
    let after = VElement::leaf(
        "s",
        NodeKind::Select {
            value: "b".to_owned(),
            options: vec![option("A", "a"), option("B", "b")],
            disabled: false,
        },
    );

    let patch = diff(&before, &after);
    assert_eq!(
        patch.ops,
        vec![
            Op::SetValue {
                key: "s",
                value: "b".to_owned(),
            },
            Op::SetOptions {
                key: "s",
                options: vec![option("A", "a"), option("B", "b")],
            },
        ],
        "значение и список пунктов обновляются раздельно"
    );
}

#[test]
fn checkbox_and_radio_diff_by_field() {
    let checkbox_before = VElement::leaf(
        "c",
        NodeKind::Checkbox {
            label: "Да".to_owned(),
            checked: false,
            disabled: false,
        },
    );
    let checkbox_after = VElement::leaf(
        "c",
        NodeKind::Checkbox {
            label: "Да".to_owned(),
            checked: true,
            disabled: false,
        },
    );
    assert_eq!(
        diff(&checkbox_before, &checkbox_after).ops,
        vec![Op::SetChecked {
            key: "c",
            checked: true,
        }]
    );

    let radio_before = VElement::leaf(
        "r",
        NodeKind::Radio {
            label: "A".to_owned(),
            value: "a".to_owned(),
            selected: false,
            disabled: false,
        },
    );
    let radio_after = VElement::leaf(
        "r",
        NodeKind::Radio {
            label: "A".to_owned(),
            value: "a".to_owned(),
            selected: true,
            disabled: false,
        },
    );
    assert_eq!(
        diff(&radio_before, &radio_after).ops,
        vec![Op::SetSelected {
            key: "r",
            selected: true,
        }]
    );
}

/// Динамические парсеры значений атрибутов совпадают по правилам с временем
/// компиляции шаблонов.
#[test]
fn runtime_parsers_match_compile_time_rules() {
    use relay::vdom::{
        Align, ButtonType, Offset, RequiredIndicator, Resize, Size, parse_align_items,
        parse_align_x, parse_align_y, parse_bool, parse_button_type, parse_color, parse_justify,
        parse_offset, parse_px, parse_required_indicator, parse_resize, parse_size_or_auto,
    };

    assert_eq!(parse_size_or_auto("auto"), Size::Auto);
    assert_eq!(parse_size_or_auto(""), Size::Auto);
    assert_eq!(parse_size_or_auto("120"), Size::Px(120.0));
    assert_eq!(parse_size_or_auto("120px"), Size::Px(120.0));
    assert_eq!(parse_size_or_auto("50%"), Size::Percent(50.0));

    assert_eq!(parse_offset("-8"), Offset::Px(-8.0));
    assert_eq!(parse_offset("12px"), Offset::Px(12.0));
    assert_eq!(parse_offset("10%"), Offset::Percent(10.0));

    assert_eq!(parse_align_x("left"), Align::Start);
    assert_eq!(parse_align_x("center"), Align::Center);
    assert_eq!(parse_align_x("right"), Align::End);
    assert_eq!(parse_align_y("top"), Align::Start);
    assert_eq!(parse_align_y("bottom"), Align::End);
    assert_eq!(parse_justify("start"), Align::Start);
    assert_eq!(parse_justify("end"), Align::End);
    assert_eq!(parse_align_items("center"), Align::Center);

    assert_eq!(parse_px("12"), 12.0);
    assert_eq!(parse_px("12px"), 12.0);

    assert_eq!(parse_color("#f00"), Some(0xFF0000FF));
    assert_eq!(parse_color("#FF0000"), Some(0xFF0000FF));
    assert_eq!(parse_color("#FF000080"), Some(0xFF000080));
    assert_eq!(parse_color("#0f08"), Some(0x00FF0088));
    assert_eq!(parse_color("0x112233"), Some(0x112233FF));

    assert!(parse_bool("true"));
    assert!(parse_bool("1"));
    assert!(parse_bool("yes"));
    assert!(!parse_bool("false"));

    assert_eq!(parse_required_indicator("*"), RequiredIndicator::Asterisk);
    assert_eq!(parse_required_indicator("none"), RequiredIndicator::None);
    assert_eq!(
        parse_required_indicator("#FF0000"),
        RequiredIndicator::Accent(0xFF0000FF)
    );

    assert_eq!(parse_button_type("button"), ButtonType::Button);
    assert_eq!(parse_button_type("reset"), ButtonType::Reset);
    assert_eq!(parse_button_type("submit"), ButtonType::Submit);

    assert_eq!(parse_resize("horizontal"), Resize::Horizontal);
    assert_eq!(parse_resize("both"), Resize::Both);
    assert_eq!(parse_resize("none"), Resize::None);
}
