#![cfg(feature = "image-support")]

//! Проверяет макрос `<image>`: встраивание PNG, размеры из IHDR и alt.

extern crate light_fw_core as light;

use light::prelude::*;
use light::vdom::{NodeKind, VElement, diff};

#[template("tests/templates/image.html")]
#[proxy]
struct ImageApp {}

fn find_image(node: &VElement) -> Option<&VElement> {
    if matches!(node.kind, NodeKind::Image { .. }) {
        return Some(node);
    }
    node.children.iter().find_map(find_image)
}

#[test]
fn image_macro_embeds_png_and_reads_dimensions() {
    let tree = ImageApp::new().render();
    let image = find_image(&tree).expect("изображение в дереве");
    match &image.kind {
        NodeKind::Image {
            src,
            data,
            width,
            height,
            alt,
        } => {
            assert_eq!(*src, "tests/templates/checker.png");
            assert_eq!((*width, *height), (4, 2));
            assert_eq!(alt, "Проверочное изображение");
            assert!(data.starts_with(b"\x89PNG\r\n\x1a\n"));
        }
        _ => unreachable!(),
    }
}

#[test]
fn image_alt_is_exposed_to_accessibility() {
    let tree = ImageApp::new().render();
    let image = find_image(&tree).expect("изображение в дереве");
    assert_eq!(image.attr("alt"), Some("Проверочное изображение"));
    #[cfg(feature = "a11y")]
    assert_eq!(
        light::a11y::roles::role_for(image),
        light::a11y::AriaRole::Image
    );
}

fn image_node(src: &'static str, data: &'static [u8], alt: &str) -> VElement {
    VElement::leaf(
        "img",
        NodeKind::Image {
            src,
            data,
            width: 1,
            height: 1,
            alt: alt.to_owned(),
        },
    )
}

#[test]
fn image_diff_marks_changed_when_data_or_alt_changes() {
    let old = image_node("a.png", b"one", "A");
    let same = image_node("a.png", b"one", "A");
    assert!(diff(&old, &same).changed.is_empty());

    let new_alt = image_node("a.png", b"one", "B");
    assert!(diff(&old, &new_alt).changed.contains("img"));

    let new_data = image_node("a.png", b"two", "A");
    assert!(diff(&old, &new_data).changed.contains("img"));
}

#[test]
fn image_diff_replaces_when_src_changes() {
    let old = image_node("a.png", b"one", "A");
    let new = image_node("b.png", b"one", "A");
    assert!(!diff(&old, &new).is_empty());
}
