//! Главный цикл отрисовки: проходим по дереву узлов и вызываем рисовальщики.

use super::canvas::Canvas;
use super::forms::{
    Interaction, draw_checkbox, draw_field_required, draw_focus_ring, draw_inline_required,
    draw_radio, mark_disabled_text, select_popup,
};
use super::menu::ellipsized;
use super::selects::{draw_grip, draw_select, draw_select_popup, render_field};
use light_vdom::layout::{CHECKBOX_BOX, CHECKBOX_GAP, FONT_SIZE, Layout, Rect};

use super::*;
use crate::window::font::measure;
pub fn render(
    layout: &Layout,
    canvas: &mut Canvas,
    font: &crate::window::font::FontMetrics,
    scale: f32,
    interaction: Interaction<'_>,
    background: u32,
) {
    let Interaction {
        focused,
        focus_ring,
        caret_pos,
        selection,
        hover,
        open_select,
        select_hover,
    } = interaction;
    #[cfg(not(feature = "elements-select"))]
    let _ = (open_select, select_hover);
    canvas.clear(background);
    let border = scale.max(1.0);
    let nodes = layout.nodes();
    // Текст внутри заблокированной кнопки рисуется приглушённым цветом.
    let mut dimmed = vec![false; nodes.len()];
    if !nodes.is_empty() {
        mark_disabled_text(nodes, 0, false, &mut dimmed);
    }
    for (index, node) in nodes.iter().enumerate() {
        let rect = node.rect;
        if !canvas.intersects(rect) {
            continue;
        }
        let style = node.style;
        let inset = style.inset().scaled(scale);
        let size = style.font_size.unwrap_or(FONT_SIZE) * scale;
        let thickness = if style.border_size > 0.0 {
            style.border_size * scale
        } else {
            border
        };
        let background = match &node.kind {
            light_vdom::NodeKind::TitleBar => Some(style.background.unwrap_or(TITLEBAR_BG)),
            light_vdom::NodeKind::Button { disabled, .. } => {
                Some(style.background.unwrap_or(if *disabled {
                    BUTTON_BG_DISABLED
                } else if hover == Some(node.key) {
                    BUTTON_BG_HOVER
                } else {
                    BUTTON_BG
                }))
            }
            light_vdom::NodeKind::TextField { .. } => Some(style.background.unwrap_or(FIELD_BG)),
            #[cfg(feature = "elements-select")]
            light_vdom::NodeKind::Select { .. } => Some(style.background.unwrap_or(FIELD_BG)),
            _ => style.background,
        };
        let draw = |canvas: &mut Canvas| {
            if let Some(color) = background {
                canvas.fill_rect(color, rect);
            }
            match &node.kind {
                light_vdom::NodeKind::Column
                | light_vdom::NodeKind::Row
                | light_vdom::NodeKind::Window => {}
                // Кастомный элемент плагина: фон (background) уже отрисован выше,
                // содержимое — как у Column (отдельные children в своём цикле),
                // дополнительная обводка/рамка не нужна.
                light_vdom::NodeKind::Custom(_) => {}
                light_vdom::NodeKind::TitleBar => {
                    if style.border_size > 0.0 {
                        if let Some(color) = style.border_color {
                            canvas.stroke_rect(color, rect, thickness);
                        }
                    } else {
                        canvas.fill_rect(
                            TITLEBAR_BORDER,
                            Rect::new(rect.x, rect.y + rect.h - border, rect.w, border),
                        );
                    }
                }
                light_vdom::NodeKind::Text { content } => {
                    let color = if style.font_color.is_none() && dimmed[index] {
                        BUTTON_LABEL_DISABLED
                    } else {
                        style.font_color.unwrap_or(COLOR_TEXT)
                    };
                    let text_rect = Rect::new(
                        rect.x + inset.left,
                        rect.y + inset.top,
                        (rect.w - inset.horizontal()).max(0.0),
                        (rect.h - inset.vertical()).max(0.0),
                    );
                    canvas.clipped(text_rect, |canvas| {
                        match style.white_space.unwrap_or_default() {
                            light_vdom::WhiteSpace::Normal => {
                                let lines = light_vdom::text_wrap::wrap_words(
                                    content,
                                    text_rect.w.max(1.0),
                                    |line| measure(font.font(), line, size).0,
                                );
                                let row_h = measure(font.font(), "", size).1.max(1.0);
                                for (index, line) in lines.iter().enumerate() {
                                    canvas.draw_text(
                                        font,
                                        line,
                                        text_rect.x,
                                        text_rect.y + index as f32 * row_h,
                                        color,
                                        size,
                                    );
                                }
                            }
                            light_vdom::WhiteSpace::Nowrap => {
                                let line = ellipsized(font, content, size, text_rect.w);
                                canvas.draw_text(
                                    font,
                                    &line,
                                    text_rect.x,
                                    text_rect.y,
                                    color,
                                    size,
                                );
                            }
                        }
                    });
                }
                #[cfg(feature = "image-support")]
                light_vdom::NodeKind::Image { src, data, .. } => {
                    if let Some(image) = super::super::image::load(src, data) {
                        let image_rect = Rect::new(
                            rect.x + inset.left,
                            rect.y + inset.top,
                            (rect.w - inset.horizontal()).max(0.0),
                            (rect.h - inset.vertical()).max(0.0),
                        );
                        canvas.draw_image(&image, image_rect);
                    }
                    if style.border_size > 0.0
                        && let Some(color) = style.border_color
                    {
                        canvas.stroke_rect(color, rect, thickness);
                    }
                }
                #[cfg(feature = "image-support")]
                light_vdom::NodeKind::RuntimeImage { image, .. } => {
                    if let Some(decoded) = super::super::image::load_runtime(image) {
                        let image_rect = Rect::new(
                            rect.x + inset.left,
                            rect.y + inset.top,
                            (rect.w - inset.horizontal()).max(0.0),
                            (rect.h - inset.vertical()).max(0.0),
                        );
                        canvas.draw_image(&decoded, image_rect);
                    }
                    if style.border_size > 0.0
                        && let Some(color) = style.border_color
                    {
                        canvas.stroke_rect(color, rect, thickness);
                    }
                }
                light_vdom::NodeKind::Button { .. } => {
                    canvas.stroke_rect(
                        style.border_color.unwrap_or(BUTTON_BORDER),
                        rect,
                        thickness,
                    );
                }
                light_vdom::NodeKind::TextField {
                    value,
                    placeholder,
                    disabled,
                    multiline,
                    resize,
                    masked,
                } => {
                    let frame = if style.invalid {
                        FIELD_INVALID
                    } else if focused == Some(node.key) {
                        FIELD_BORDER_FOCUS
                    } else {
                        style.border_color.unwrap_or(FIELD_BORDER)
                    };
                    let frame_thickness = if style.invalid {
                        (2.0 * scale).max(thickness)
                    } else {
                        thickness
                    };
                    canvas.stroke_rect(frame, rect, frame_thickness);
                    let display;
                    let (text, color) = if value.is_empty() {
                        (placeholder.as_str(), COLOR_MUTED)
                    } else if *masked {
                        display = "\u{2022}".repeat(value.chars().count());
                        (display.as_str(), style.font_color.unwrap_or(COLOR_TEXT))
                    } else {
                        (value.as_str(), style.font_color.unwrap_or(COLOR_TEXT))
                    };
                    let active = focused == Some(node.key) && !*disabled;
                    render_field(
                        canvas,
                        font,
                        rect,
                        inset,
                        scale,
                        size,
                        border,
                        text,
                        color,
                        *multiline,
                        active,
                        caret_pos,
                        if active { selection } else { None },
                    );
                    if *resize != light_vdom::Resize::None {
                        draw_grip(canvas, rect, scale);
                    }
                    if style.required {
                        draw_field_required(
                            canvas,
                            font,
                            rect,
                            scale,
                            size,
                            style.required_indicator,
                        );
                    }
                }
                #[cfg(feature = "elements-checkbox")]
                light_vdom::NodeKind::Checkbox {
                    label,
                    checked,
                    disabled,
                } => {
                    draw_checkbox(
                        canvas, font, rect, inset, scale, size, label, *checked, *disabled, style,
                    );
                    if style.required {
                        let end = rect.x
                            + inset.left
                            + CHECKBOX_BOX * scale
                            + CHECKBOX_GAP * scale
                            + measure(font.font(), label, size).0;
                        draw_inline_required(
                            canvas,
                            font,
                            end,
                            rect,
                            scale,
                            size,
                            style.required_indicator,
                        );
                    }
                }
                #[cfg(feature = "elements-radio")]
                light_vdom::NodeKind::Radio {
                    label,
                    selected,
                    disabled,
                    ..
                } => {
                    draw_radio(
                        canvas, font, rect, inset, scale, size, label, *selected, *disabled, style,
                    );
                    if style.required {
                        let end = rect.x
                            + inset.left
                            + CHECKBOX_BOX * scale
                            + CHECKBOX_GAP * scale
                            + measure(font.font(), label, size).0;
                        draw_inline_required(
                            canvas,
                            font,
                            end,
                            rect,
                            scale,
                            size,
                            style.required_indicator,
                        );
                    }
                }
                #[cfg(feature = "elements-select")]
                light_vdom::NodeKind::Select {
                    value,
                    options,
                    disabled,
                } => {
                    draw_select(
                        canvas,
                        font,
                        rect,
                        inset,
                        scale,
                        size,
                        thickness,
                        value,
                        options,
                        *disabled,
                        style,
                        open_select == Some(node.key),
                    );
                    if style.required {
                        draw_field_required(
                            canvas,
                            font,
                            rect,
                            scale,
                            size,
                            style.required_indicator,
                        );
                    }
                }
            }
        };
        if let Some(clip) = node.clip_rect {
            canvas.clipped(clip, draw);
        } else {
            draw(canvas);
        }
    }

    for node in nodes {
        let vertical = node.overflow_y.scrollable() && node.max_scroll_y() > 0.5;
        let horizontal = node.overflow_x.scrollable() && node.max_scroll_x() > 0.5;
        if !vertical && !horizontal {
            // Прокрутки больше нет: закрашиваем бывшую полосу фоном
            // контейнера, чтобы на экране не осталась «фантомная» полоса.
            if let Some(color) = node.style.background {
                let rect = node.rect;
                let bar = SCROLLBAR_SIZE * scale;
                let draw = |canvas: &mut Canvas| {
                    if node.overflow_y.scrollable() {
                        canvas.fill_rect(
                            color,
                            Rect::new(rect.x + rect.w - bar, rect.y, bar, rect.h),
                        );
                    }
                    if node.overflow_x.scrollable() {
                        let width = if node.overflow_y.scrollable() {
                            (rect.w - bar).max(0.0)
                        } else {
                            rect.w
                        };
                        canvas
                            .fill_rect(color, Rect::new(rect.x, rect.y + rect.h - bar, width, bar));
                    }
                };
                if let Some(clip) = node.clip_rect {
                    canvas.clipped(clip, draw);
                } else {
                    draw(canvas);
                }
            }
            continue;
        }
        let rect = node.rect;
        if !canvas.intersects(rect) {
            continue;
        }
        let draw = |canvas: &mut Canvas| {
            let bar = SCROLLBAR_SIZE * scale;
            let min_thumb = (SCROLLBAR_MIN_THUMB * scale).min(bar * 2.0);
            if vertical {
                let track = Rect::new(rect.x + rect.w - bar, rect.y, bar, rect.h);
                canvas.fill_rect(SCROLLBAR_TRACK, track);
                let thumb_h =
                    (track.h * rect.h / node.scroll_height).clamp(min_thumb.min(track.h), track.h);
                let travel = track.h - thumb_h;
                let position = if node.max_scroll_y() > 0.0 {
                    node.scroll_y / node.max_scroll_y()
                } else {
                    0.0
                };
                canvas.fill_rect(
                    SCROLLBAR_THUMB,
                    Rect::new(track.x, track.y + travel * position, bar, thumb_h),
                );
            }
            if horizontal {
                let width = if vertical {
                    (rect.w - bar).max(0.0)
                } else {
                    rect.w
                };
                let track = Rect::new(rect.x, rect.y + rect.h - bar, width, bar);
                canvas.fill_rect(SCROLLBAR_TRACK, track);
                let thumb_w =
                    (track.w * rect.w / node.scroll_width).clamp(min_thumb.min(track.w), track.w);
                let travel = track.w - thumb_w;
                let position = if node.max_scroll_x() > 0.0 {
                    node.scroll_x / node.max_scroll_x()
                } else {
                    0.0
                };
                canvas.fill_rect(
                    SCROLLBAR_THUMB,
                    Rect::new(track.x + travel * position, track.y, thumb_w, bar),
                );
            }
        };
        if let Some(clip) = node.clip_rect {
            canvas.clipped(clip, draw);
        } else {
            draw(canvas);
        }
    }

    // Кольцо фокуса рисуется поверх дерева, вокруг узла с клавиатурным
    // фокусом. Текстовые поля имеют собственную рамку фокуса.
    if let Some(key) = focus_ring
        && let Some(node) = layout.node(key)
        && !matches!(node.kind, light_vdom::NodeKind::TextField { .. })
    {
        draw_focus_ring(canvas, node.rect, scale);
    }

    #[cfg(feature = "elements-select")]
    if let Some(key) = open_select
        && let Some(node) = layout.node(key)
        && let light_vdom::NodeKind::Select { value, options, .. } = &node.kind
        && !options.is_empty()
    {
        let size = node.style.font_size.unwrap_or(FONT_SIZE) * scale;
        draw_select_popup(
            canvas,
            font,
            select_popup(node.rect, options.len(), scale),
            options,
            value,
            select_hover,
            scale,
            size,
        );
    }
}
