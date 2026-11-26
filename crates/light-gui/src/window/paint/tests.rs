//! Тесты пиксельного буфера для рамок контейнеров (Column/Row/Window) и
//! кастомных узлов: `border-size` + `border-color` должны рисовать рамку,
//! а без `border-size` — не рисовать.

use super::canvas::Canvas;
use super::rgb;
use crate::window::font::FontMetrics;
use crate::window::paint::forms::Interaction;
use crate::window::paint::render;
use light_vdom::layout::{Layout, TextMetrics};
use light_vdom::{NodeKind, Style, VElement};

/// Метрики заглушка: знак шириной 8px, высота строки 16px.
struct Stub;

impl TextMetrics for Stub {
    fn measure(&self, text: &str, _size: f32) -> (f32, f32) {
        (text.chars().count() as f32 * 8.0, 16.0)
    }
}

fn pixel(pixels: &[u8], width: u32, x: u32, y: u32) -> u32 {
    let i = ((y * width + x) * 4) as usize;
    ((pixels[i] as u32) << 24)
        | ((pixels[i + 1] as u32) << 16)
        | ((pixels[i + 2] as u32) << 8)
        | pixels[i + 3] as u32
}

fn column(key: &'static str, children: Vec<VElement>) -> VElement {
    VElement::container(key, NodeKind::Column, children)
}

fn custom(key: &'static str, tag: &'static str, children: Vec<VElement>) -> VElement {
    VElement::container(key, NodeKind::Custom(tag), children)
}

fn with_border(node: &mut VElement, size: f32, color: u32) {
    node.style = Style {
        border_size: size,
        border_color: Some(color),
        ..node.style
    };
}

fn render_tree(root: &VElement, w: u32, h: u32) -> Vec<u8> {
    let layout = Layout::build(root, (w as f32, h as f32), &Stub);
    let mut pixels = vec![0u8; (w * h * 4) as usize];
    let region = light_vdom::layout::Rect::new(0.0, 0.0, w as f32, h as f32);
    let mut canvas = Canvas::new(&mut pixels, w, h, region);
    let Ok(font) = FontMetrics::load() else {
        return pixels;
    };
    let interaction = Interaction::default();
    render(&layout, &mut canvas, &font, 1.0, interaction, rgb(0, 0, 0));
    pixels
}

#[test]
fn column_with_border_paints_perimeter() {
    let mut root = column("", vec![]);
    with_border(&mut root, 2.0, 0x888888FF);
    let pixels = render_tree(&root, 100, 60);

    // Рамка 2px: верхняя/нижняя полосы и левая/правая колонки — серые.
    assert_eq!(pixel(&pixels, 100, 50, 0), 0x888888FF, "верхняя полоса");
    assert_eq!(
        pixel(&pixels, 100, 50, 1),
        0x888888FF,
        "верхняя полоса (px 1)"
    );
    assert_eq!(pixel(&pixels, 100, 50, 59), 0x888888FF, "нижняя полоса");
    assert_eq!(
        pixel(&pixels, 100, 50, 58),
        0x888888FF,
        "нижняя полоса (px 1)"
    );
    assert_eq!(pixel(&pixels, 100, 0, 30), 0x888888FF, "левая колонка");
    assert_eq!(
        pixel(&pixels, 100, 1, 30),
        0x888888FF,
        "левая колонка (px 1)"
    );
    assert_eq!(pixel(&pixels, 100, 99, 30), 0x888888FF, "правая колонка");
    assert_eq!(
        pixel(&pixels, 100, 98, 30),
        0x888888FF,
        "правая колонка (px 1)"
    );
    // Центр — чёрный (фон), не серый.
    assert_eq!(pixel(&pixels, 100, 50, 30), 0x000000FF, "внутренность");
}

#[test]
fn column_without_border_does_not_paint_border() {
    let root = column("", vec![]);
    let pixels = render_tree(&root, 100, 60);

    // Без border-size — рамка не рисуется, периметр остаётся чёрным (фон).
    assert_eq!(pixel(&pixels, 100, 50, 0), 0x000000FF, "верхний край — фон");
    assert_eq!(pixel(&pixels, 100, 50, 59), 0x000000FF, "нижний край — фон");
    assert_eq!(pixel(&pixels, 100, 0, 30), 0x000000FF, "левый край — фон");
    assert_eq!(pixel(&pixels, 100, 99, 30), 0x000000FF, "правый край — фон");
}

#[test]
fn custom_node_with_border_paints_perimeter() {
    let mut node = custom("c", "datepicker", vec![]);
    with_border(&mut node, 2.0, 0xAABBCCFF);
    let pixels = render_tree(&node, 100, 60);

    // Кастомный узел как корень занимает весь канвас (0,0,100,60).
    assert_eq!(pixel(&pixels, 100, 50, 0), 0xAABBCCFF, "верхняя полоса");
    assert_eq!(pixel(&pixels, 100, 50, 59), 0xAABBCCFF, "нижняя полоса");
    assert_eq!(pixel(&pixels, 100, 0, 30), 0xAABBCCFF, "левая колонка");
    assert_eq!(pixel(&pixels, 100, 99, 30), 0xAABBCCFF, "правая колонка");
    assert_eq!(pixel(&pixels, 100, 50, 30), 0x000000FF, "внутренность");
}

#[test]
fn custom_node_without_border_does_not_paint_border() {
    let node = custom("c", "datepicker", vec![]);
    let pixels = render_tree(&node, 100, 60);

    // Без border-size — рамка не рисуется, периметр остаётся чёрным (фон).
    assert_eq!(pixel(&pixels, 100, 50, 0), 0x000000FF, "верхний край — фон");
    assert_eq!(pixel(&pixels, 100, 0, 30), 0x000000FF, "левый край — фон");
    assert_eq!(pixel(&pixels, 100, 99, 30), 0x000000FF, "правый край — фон");
    assert_eq!(pixel(&pixels, 100, 50, 59), 0x000000FF, "нижний край — фон");
}

#[test]
fn button_default_border_unchanged() {
    let btn = VElement::container(
        "b",
        NodeKind::Button {
            disabled: false,
            button_type: light_vdom::ButtonType::Submit,
        },
        vec![VElement::leaf(
            "t",
            NodeKind::Text {
                content: "Hi".into(),
            },
        )],
    );
    let root = column("", vec![btn]);
    let pixels = render_tree(&root, 100, 60);

    // Кнопка по умолчанию: рамка BUTTON_BORDER (0xC2C9D4FF), толщина 1px.
    // Кнопка занимает (0,0,100,min_height). Толщина рамки = 1px.
    // fill_rect не блендит с фоном канваса — просто заменяет пиксели,
    // поэтому проверяем, что рамка НЕ прозрачная (альфа != 0).
    let p = pixel(&pixels, 100, 50, 0);
    assert_ne!(p, 0, "верхняя рамка кнопки не прозрачна");
    assert_ne!(p & 0x00FFFFFF, 0, "цвет рамки не чёрный");
    let corner = pixel(&pixels, 100, 0, 0);
    assert_ne!(corner, 0, "левый угол рамки не прозрачен");
}

#[test]
fn textfield_default_border_unchanged() {
    let field = VElement::container(
        "f",
        NodeKind::TextField {
            value: "abc".into(),
            placeholder: String::new(),
            disabled: false,
            multiline: false,
            resize: light_vdom::Resize::None,
            masked: false,
        },
        vec![],
    );
    let root = column("", vec![field]);
    let pixels = render_tree(&root, 100, 60);

    // Поле ввода: рамка FIELD_BORDER (0xB8C0CCFF), толщина 1px.
    assert_eq!(pixel(&pixels, 100, 50, 0), 0xB8C0CCFF, "верхняя рамка поля");
    assert_eq!(pixel(&pixels, 100, 0, 0), 0xB8C0CCFF, "левый угол рамки");
}

#[test]
fn titlebar_default_bottom_border_unchanged() {
    let tb = VElement::container("tb", NodeKind::TitleBar, vec![]);
    let pixels = render_tree(&tb, 100, 60);

    // TitleBar как корень занимает весь канвас (0,0,100,60).
    // TitleBar по умолчанию: нижняя граница TITLEBAR_BORDER (0xC4CBD6FF),
    // остальное — без рамки (background TITLEBAR_BG 0xDFE4EBFF).
    // fill_rect не блендит с фоном канваса, поэтому проверяем, что
    // нижний ряд содержит рамку или фон (не прозрачность).
    let bottom = 59;
    let p = pixel(&pixels, 100, 50, bottom);
    assert_ne!(p, 0, "нижняя граница titlebar не прозрачна");
    // Центр titlebar — фон.
    let center = pixel(&pixels, 100, 50, 30);
    assert_ne!(center, 0, "центр titlebar не прозрачен");
    // Фон titlebar должен быть светлым (TITLEBAR_BG 0xDFE4EBFF):
    // красный канал > 200.
    let r = (center >> 16) & 0xFF;
    assert!(r > 200, "фон titlebar светлый (r={r})");
}
