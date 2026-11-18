//! Отрисовка форм: поля, флажки, переключатели и вспомогательные функции.

use super::canvas::Canvas;
use light_vdom::layout::{CHECKBOX_BOX, CHECKBOX_GAP, PlacedNode, Rect};

use super::{COLOR_MUTED, COLOR_TEXT, FIELD_BORDER, FOCUS_RING, FOCUS_RING_OFFSET, FOCUS_RING_WIDTH, FORM_ACCENT, FORM_BG, MENU_ROW_H, REQUIRED_COLOR};

#[derive(Debug, Clone, Copy, Default)]
pub struct Interaction<'a> {
    /// Ключ текстового поля в фокусе.
    pub focused: Option<&'a str>,
    /// Ключ узла с клавиатурным фокусом (для кольца фокуса).
    pub focus_ring: Option<&'a str>,
    /// Позиция каретки (в символах) для поля в фокусе.
    pub caret_pos: usize,
    /// Выделенный диапазон (в символах, начало ≤ конец) для поля в фокусе.
    pub selection: Option<(usize, usize)>,
    /// Ключ кнопки под курсором.
    pub hover: Option<&'a str>,
    /// Ключ раскрытого выпадающего списка.
    pub open_select: Option<&'a str>,
    /// Индекс пункта раскрытого списка под курсором.
    pub select_hover: Option<usize>,
}

/// Прямоугольник раскрытого списка под его полем.
#[cfg(feature = "elements-select")]
pub(crate) fn select_popup(rect: Rect, count: usize, scale: f32) -> Rect {
    Rect::new(
        rect.x,
        rect.y + rect.h,
        rect.w,
        count as f32 * MENU_ROW_H * scale,
    )
}

/// Помечает узлы, находящиеся внутри заблокированной кнопки, чтобы их текст
/// рисовался приглушённым цветом.
pub(crate) fn mark_disabled_text(nodes: &[PlacedNode], index: usize, inherited: bool, out: &mut [bool]) {
    let disabled = inherited
        || matches!(
            nodes[index].kind,
            light_vdom::NodeKind::Button { disabled: true, .. }
        );
    out[index] = disabled;
    for &child in &nodes[index].children {
        mark_disabled_text(nodes, child, disabled, out);
    }
}

/// Отрисовывает разложенное дерево в область холста.
///
/// Узлы идут в порядке раскладки (родители раньше детей, поздние соседи выше
/// ранних), поэтому наложение сохраняется. Фон области задаётся `background`
/// (цвет `<window>` с альфой). Каретка и выделение рисуются для поля в фокусе,
/// подсветка — для кнопки под курсором.
pub(crate) fn draw_focus_ring(canvas: &mut Canvas, rect: Rect, scale: f32) {
    let offset = FOCUS_RING_OFFSET * scale;
    let thickness = FOCUS_RING_WIDTH * scale;
    let ring = Rect::new(
        rect.x + offset,
        rect.y + offset,
        (rect.w - 2.0 * offset).max(0.0),
        (rect.h - 2.0 * offset).max(0.0),
    );
    canvas.stroke_rect(FOCUS_RING, ring, thickness);
}

/// Рисует индикатор обязательности текстового поля/списка в правом верхнем углу.
pub(crate) fn draw_field_required(
    canvas: &mut Canvas,
    font: &crate::window::font::FontMetrics,
    rect: Rect,
    scale: f32,
    size: f32,
    indicator: light_vdom::RequiredIndicator,
) {
    match indicator {
        light_vdom::RequiredIndicator::None => {}
        light_vdom::RequiredIndicator::Asterisk => {
            let (width, _) = measure(font.font(), "*", size);
            canvas.draw_text(
                font,
                "*",
                rect.x + rect.w - width - 4.0 * scale,
                rect.y + 1.0 * scale,
                REQUIRED_COLOR,
                size,
            );
        }
        light_vdom::RequiredIndicator::Accent(color) => {
            let width = (2.0 * scale).max(2.0);
            canvas.fill_rect(color, Rect::new(rect.x, rect.y, width, rect.h));
        }
    }
}

/// Рисует индикатор обязательности флажка/переключателя после подписи.
#[cfg(any(feature = "elements-checkbox", feature = "elements-radio"))]
pub(crate) fn draw_inline_required(
    canvas: &mut Canvas,
    font: &crate::window::font::FontMetrics,
    text_end_x: f32,
    rect: Rect,
    scale: f32,
    size: f32,
    indicator: light_vdom::RequiredIndicator,
) {
    match indicator {
        light_vdom::RequiredIndicator::None => {}
        light_vdom::RequiredIndicator::Asterisk => {
            let (_, text_h) = measure(font.font(), "*", size);
            canvas.draw_text(
                font,
                "*",
                text_end_x + 4.0 * scale,
                rect.y + (rect.h - text_h) / 2.0,
                REQUIRED_COLOR,
                size,
            );
        }
        light_vdom::RequiredIndicator::Accent(color) => {
            let width = (2.0 * scale).max(2.0);
            canvas.fill_rect(color, Rect::new(rect.x, rect.y, width, rect.h));
        }
    }
}

/// Рисует флажок с подписью.
#[allow(clippy::too_many_arguments)]
#[cfg(feature = "elements-checkbox")]
pub(crate) fn draw_checkbox(
    canvas: &mut Canvas,
    font: &crate::window::font::FontMetrics,
    rect: Rect,
    inset: light_vdom::Edges,
    scale: f32,
    size: f32,
    label: &str,
    checked: bool,
    disabled: bool,
    style: light_vdom::Style,
) {
    let box_size = CHECKBOX_BOX * scale;
    let gap = CHECKBOX_GAP * scale;
    let left = rect.x + inset.left;
    let top = rect.y + inset.top + (rect.h - inset.vertical() - box_size).max(0.0) / 2.0;
    let box_rect = Rect::new(left, top, box_size, box_size);
    canvas.fill_rect(FORM_BG, box_rect);
    canvas.stroke_rect(FIELD_BORDER, box_rect, scale.max(1.0));
    if checked {
        canvas.fill_rect(FORM_ACCENT, box_rect);
        let stroke = (box_size * 0.14).max(1.0);
        let p1 = (box_rect.x + box_size * 0.22, box_rect.y + box_size * 0.52);
        let p2 = (box_rect.x + box_size * 0.42, box_rect.y + box_size * 0.72);
        let p3 = (box_rect.x + box_size * 0.78, box_rect.y + box_size * 0.28);
        canvas.draw_line(FORM_BG, p1, p2, stroke);
        canvas.draw_line(FORM_BG, p2, p3, stroke);
    }
    let color = if disabled {
        COLOR_MUTED
    } else {
        style.font_color.unwrap_or(COLOR_TEXT)
    };
    let (_, text_h) = measure(font.font(), label, size);
    canvas.draw_text(
        font,
        label,
        left + box_size + gap,
        rect.y + (rect.h - text_h) / 2.0,
        color,
        size,
    );
}

/// Рисует переключатель с подписью.
#[allow(clippy::too_many_arguments)]
#[cfg(feature = "elements-radio")]
use crate::window::font::measure;
pub(crate) fn draw_radio(
    canvas: &mut Canvas,
    font: &crate::window::font::FontMetrics,
    rect: Rect,
    inset: light_vdom::Edges,
    scale: f32,
    size: f32,
    label: &str,
    selected: bool,
    disabled: bool,
    style: light_vdom::Style,
) {
    let box_size = CHECKBOX_BOX * scale;
    let gap = CHECKBOX_GAP * scale;
    let left = rect.x + inset.left;
    let cx = left + box_size / 2.0;
    let cy = rect.y + rect.h / 2.0;
    let radius = box_size / 2.0;
    canvas.fill_circle(FORM_BG, cx, cy, radius);
    canvas.fill_circle(FIELD_BORDER, cx, cy, radius);
    canvas.fill_circle(FORM_BG, cx, cy, (radius - scale.max(1.0)).max(0.0));
    if selected {
        canvas.fill_circle(FORM_ACCENT, cx, cy, radius * 0.55);
    }
    let color = if disabled {
        COLOR_MUTED
    } else {
        style.font_color.unwrap_or(COLOR_TEXT)
    };
    let (_, text_h) = measure(font.font(), label, size);
    canvas.draw_text(
        font,
        label,
        left + box_size + gap,
        rect.y + (rect.h - text_h) / 2.0,
        color,
        size,
    );
}
