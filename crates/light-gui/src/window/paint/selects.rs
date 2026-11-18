//! Отрисовка выпающих списков и элементов выбора.

use super::canvas::Canvas;
use light_vdom::layout::{CHECKBOX_BOX, CHECKBOX_GAP, FIELD_PAD_X, Rect};
#[cfg(feature = "elements-select")]
use light_vdom::layout::SELECT_PAD_X;

use super::*;

pub(crate) fn draw_select(
    canvas: &mut Canvas,
    font: &crate::window::font::FontMetrics,
    rect: Rect,
    inset: light_vdom::Edges,
    scale: f32,
    size: f32,
    thickness: f32,
    value: &str,
    options: &[light_vdom::SelectOption],
    disabled: bool,
    style: light_vdom::Style,
    open: bool,
) {
    let frame = if style.invalid {
        FIELD_INVALID
    } else if open {
        FORM_ACCENT
    } else {
        style.border_color.unwrap_or(FIELD_BORDER)
    };
    let thickness = if style.invalid {
        (2.0 * scale).max(thickness)
    } else {
        thickness
    };
    canvas.stroke_rect(frame, rect, thickness);
    let label = options
        .iter()
        .find(|option| option.value == value)
        .map(|option| option.label.as_str())
        .unwrap_or(value);
    let color = if disabled {
        COLOR_MUTED
    } else {
        style.font_color.unwrap_or(COLOR_TEXT)
    };
    let (_, text_h) = measure(font.font(), label, size);
    let text_rect = Rect::new(
        rect.x + inset.left,
        rect.y,
        (rect.w - inset.horizontal() - 2.0 * SELECT_PAD_X * scale - CHECKBOX_BOX * scale).max(0.0),
        rect.h,
    );
    canvas.clipped(text_rect, |c| {
        c.draw_text(
            font,
            label,
            rect.x + inset.left + SELECT_PAD_X * scale,
            rect.y + (rect.h - text_h) / 2.0,
            color,
            size,
        );
    });
    let cx = rect.x + rect.w - inset.right - CHECKBOX_BOX * scale / 2.0;
    let cy = rect.y + rect.h / 2.0;
    let width = (CHECKBOX_BOX * scale * 0.5).max(4.0);
    let stroke = (scale * 1.5).max(1.0);
    canvas.draw_line(
        frame,
        (cx - width / 2.0, cy - CHECKBOX_GAP * scale / 4.0),
        (cx, cy + CHECKBOX_GAP * scale / 4.0),
        stroke,
    );
    canvas.draw_line(
        frame,
        (cx, cy + CHECKBOX_GAP * scale / 4.0),
        (cx + width / 2.0, cy - CHECKBOX_GAP * scale / 4.0),
        stroke,
    );
}

/// Рисует раскрытое меню выпадающего списка поверх интерфейса.
#[allow(clippy::too_many_arguments)]
#[cfg(feature = "elements-select")]
use crate::window::font::measure;
pub(crate) fn draw_select_popup(
    canvas: &mut Canvas,
    font: &crate::window::font::FontMetrics,
    rect: Rect,
    options: &[light_vdom::SelectOption],
    value: &str,
    hover: Option<usize>,
    scale: f32,
    size: f32,
) {
    let border = MENU_BORDER * scale;
    canvas.fill_rect(MENU_BG, rect);
    canvas.stroke_rect(MENU_BORDER_COLOR, rect, border);
    let row = MENU_ROW_H * scale;
    let pad = SELECT_PAD_X * scale;
    for (index, option) in options.iter().enumerate() {
        let y = rect.y + border + index as f32 * row;
        if hover == Some(index) {
            canvas.fill_rect(
                MENU_HOVER,
                Rect::new(rect.x + border, y, (rect.w - 2.0 * border).max(0.0), row),
            );
        }
        let color = if option.value == value {
            FORM_ACCENT
        } else {
            MENU_TEXT
        };
        let (_, text_h) = measure(font.font(), &option.label, size);
        canvas.draw_text(
            font,
            &option.label,
            rect.x + pad,
            y + (row - text_h) / 2.0,
            color,
            size,
        );
    }
}

/// Рисует содержимое текстового поля с обрезкой по его границам.
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_field(
    canvas: &mut Canvas,
    font: &crate::window::font::FontMetrics,
    rect: Rect,
    inset: light_vdom::Edges,
    scale: f32,
    size: f32,
    border: f32,
    text: &str,
    color: u32,
    multiline: bool,
    focused: bool,
    caret_pos: usize,
    selection: Option<(usize, usize)>,
) {
    let pad_x = FIELD_PAD_X * scale;
    let text_left = rect.x + inset.left + pad_x;
    let text_max_w = (rect.w - inset.horizontal() - 2.0 * pad_x).max(1.0);
    let row_h = crate::window::text::row_height(font, size);
    let line_box_h = measure(font.font(), "", size).1;
    let inner = Rect::new(
        rect.x + border,
        rect.y + border,
        (rect.w - 2.0 * border).max(0.0),
        (rect.h - 2.0 * border).max(0.0),
    );
    let lines = crate::window::text::layout(font, text, size, text_max_w, multiline);

    if multiline {
        let content = Rect::new(
            rect.x + inset.left,
            rect.y + inset.top,
            (rect.w - inset.horizontal()).max(0.0),
            (rect.h - inset.vertical()).max(0.0),
        );
        let visible = (content.h / row_h).floor().max(1.0);
        let caret_line = crate::window::text::line_of(&lines, caret_pos);
        let scroll_y = ((caret_line as f32 + 1.0 - visible).max(0.0)) * row_h;
        canvas.clipped(inner, |c| {
            for (index, line) in lines.iter().enumerate() {
                let y = content.y + index as f32 * row_h - scroll_y;
                draw_selection(c, font, line, selection, text_left, y, row_h, size);
                c.draw_text(
                    font,
                    &line.text,
                    text_left,
                    y + (row_h - line_box_h) / 2.0,
                    color,
                    size,
                );
            }
        });
        if focused {
            let line = &lines[caret_line.min(lines.len() - 1)];
            let offset = caret_pos.saturating_sub(line.start);
            let px = crate::window::text::prefix_width(font, &line.text, size, offset);
            let y = content.y + caret_line as f32 * row_h - scroll_y;
            canvas.clipped(inner, |c| {
                c.fill_rect(
                    COLOR_CARET,
                    Rect::new(
                        text_left + px + scale,
                        y + 2.0 * scale,
                        border,
                        row_h - 4.0 * scale,
                    ),
                );
            });
        }
    } else {
        let full = &lines[0];
        let caret_prefix = if focused {
            crate::window::text::prefix_width(font, text, size, caret_pos.min(text.chars().count()))
        } else {
            0.0
        };
        let max_scroll = (full.width - text_max_w).max(0.0);
        let scroll_x = (caret_prefix + 2.0 * scale - text_max_w).clamp(0.0, max_scroll);
        let y = rect.y + (rect.h - line_box_h) / 2.0;
        canvas.clipped(inner, |c| {
            draw_selection(
                c,
                font,
                full,
                selection,
                text_left - scroll_x,
                rect.y + (rect.h - row_h) / 2.0,
                row_h,
                size,
            );
            c.draw_text(font, text, text_left - scroll_x, y, color, size);
        });
        if focused {
            canvas.clipped(inner, |c| {
                c.fill_rect(
                    COLOR_CARET,
                    Rect::new(
                        text_left + caret_prefix - scroll_x + scale,
                        rect.y + 5.0 * scale,
                        border,
                        rect.h - 10.0 * scale,
                    ),
                );
            });
        }
    }
}

/// Рисует подсветку выделения для одной визуальной строки.
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_selection(
    canvas: &mut Canvas,
    font: &crate::window::font::FontMetrics,
    line: &crate::window::text::Line,
    selection: Option<(usize, usize)>,
    text_left: f32,
    y: f32,
    height: f32,
    size: f32,
) {
    let Some((start, end)) = selection else {
        return;
    };
    let line_len = line.text.chars().count();
    let line_end = line.start + line_len;
    let from = start.max(line.start);
    let to = end.min(line_end);
    if from >= to {
        return;
    }
    let x0 = text_left + crate::window::text::prefix_width(font, &line.text, size, from - line.start);
    let x1 = text_left + crate::window::text::prefix_width(font, &line.text, size, to - line.start);
    canvas.fill_rect(SELECTION_BG, Rect::new(x0, y, x1 - x0, height));
}

/// Рисует индикатор изменения размера в правом нижнем углу поля.
pub(crate) fn draw_grip(canvas: &mut Canvas, rect: Rect, scale: f32) {
    let unit = 2.0 * scale;
    let right = rect.x + rect.w;
    let bottom = rect.y + rect.h;
    for step in 1..=3 {
        let offset = step as f32 * 3.0 * scale;
        canvas.fill_rect(
            RESIZE_GRIP,
            Rect::new(right - offset - unit, bottom - offset - unit, unit, unit),
        );
    }
}
