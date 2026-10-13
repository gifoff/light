//! Тесты выбора курсора мыши: значение по роли узла, явный атрибут `cursor`
//! и «всплытие» к ближайшему подходящему родителю.
#![cfg(feature = "window")]

use relay::layout::{Layout, Point, TextMetrics};
use relay::vdom::{ButtonType, Cursor, NodeKind, Resize, VElement};
use relay::window::{cursor_for, default_cursor_for, resolve_cursor};

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
            label: "Кнопка".to_owned(),
            disabled,
            button_type: ButtonType::Submit,
        },
    )
}

fn text_field(key: &'static str, disabled: bool) -> VElement {
    VElement::leaf(
        key,
        NodeKind::TextField {
            value: String::new(),
            placeholder: String::new(),
            disabled,
            multiline: false,
            resize: Resize::None,
            masked: false,
        },
    )
}

fn text(key: &'static str, content: &str) -> VElement {
    VElement::leaf(
        key,
        NodeKind::Text {
            content: content.to_owned(),
        },
    )
}

fn with_attr(mut node: VElement, name: &'static str, value: &str) -> VElement {
    node.attrs.push((name, value.to_owned()));
    node
}

#[test]
fn from_name_accepts_known_values() {
    assert_eq!(Cursor::from_name("pointer"), Some(Cursor::Pointer));
    assert_eq!(Cursor::from_name("NOT-ALLOWED"), Some(Cursor::NotAllowed));
    assert_eq!(Cursor::from_name("  ew-resize "), Some(Cursor::EwResize));
    assert_eq!(Cursor::from_name("default"), Some(Cursor::Default));
}

#[test]
fn from_name_rejects_unknown_values() {
    assert_eq!(Cursor::from_name("hand"), None);
    assert_eq!(Cursor::from_name(""), None);
}

#[test]
fn default_cursor_matches_role() {
    assert_eq!(default_cursor_for(&button("b", false)), Cursor::Pointer);
    assert_eq!(default_cursor_for(&text_field("t", false)), Cursor::Text);
    assert_eq!(default_cursor_for(&text("x", "подпись")), Cursor::Default);
}

#[test]
fn disabled_controls_show_not_allowed() {
    assert_eq!(default_cursor_for(&button("b", true)), Cursor::NotAllowed);
    assert_eq!(
        default_cursor_for(&text_field("t", true)),
        Cursor::NotAllowed
    );
}

#[test]
fn link_roles_and_anchor_tag_show_pointer() {
    let role = with_attr(
        VElement::container("l", NodeKind::Column, Vec::new()),
        "role",
        "link",
    );
    let anchor = VElement::container("a", NodeKind::Column, Vec::new()).with_tag("a");
    assert_eq!(default_cursor_for(&role), Cursor::Pointer);
    assert_eq!(default_cursor_for(&anchor), Cursor::Pointer);
}

#[test]
fn explicit_cursor_overrides_role() {
    let field = with_attr(text_field("t", false), "cursor", "move");
    assert_eq!(cursor_for(&field), Cursor::Move);
    let forced = with_attr(button("b", false), "cursor", "default");
    assert_eq!(cursor_for(&forced), Cursor::Default);
}

#[test]
fn cursor_bubbles_to_linking_parent() {
    let link = with_attr(
        VElement::container("link", NodeKind::Column, vec![text("label", "Ссылка")]),
        "role",
        "link",
    );
    let root = VElement::container("root", NodeKind::Column, vec![link]);
    let layout = Layout::build(&root, (400.0, 400.0), &StubMetrics);
    let point = center(&layout, "label");

    assert_eq!(resolve_cursor(&layout, &root, point), Cursor::Pointer);
}

#[test]
fn explicit_cursor_on_child_wins_over_parent() {
    let child = with_attr(text("label", "Текст"), "cursor", "wait");
    let link = with_attr(
        VElement::container("link", NodeKind::Column, vec![child]),
        "role",
        "link",
    );
    let root = VElement::container("root", NodeKind::Column, vec![link]);
    let layout = Layout::build(&root, (400.0, 400.0), &StubMetrics);
    let point = center(&layout, "label");

    assert_eq!(resolve_cursor(&layout, &root, point), Cursor::Wait);
}

#[test]
fn explicit_default_stops_bubbling() {
    let child = with_attr(text("label", "Текст"), "cursor", "default");
    let link = with_attr(
        VElement::container("link", NodeKind::Column, vec![child]),
        "role",
        "link",
    );
    let root = VElement::container("root", NodeKind::Column, vec![link]);
    let layout = Layout::build(&root, (400.0, 400.0), &StubMetrics);
    let point = center(&layout, "label");

    assert_eq!(resolve_cursor(&layout, &root, point), Cursor::Default);
}

#[test]
fn disabled_button_resolves_to_not_allowed() {
    let root = VElement::container("root", NodeKind::Column, vec![button("b", true)]);
    let layout = Layout::build(&root, (400.0, 400.0), &StubMetrics);
    let point = center(&layout, "b");

    assert_eq!(resolve_cursor(&layout, &root, point), Cursor::NotAllowed);
}

fn center(layout: &Layout, key: &str) -> Point {
    let rect = layout.rect_of(key).expect("узел размещён");
    Point::new(rect.x + rect.w / 2.0, rect.y + rect.h / 2.0)
}
