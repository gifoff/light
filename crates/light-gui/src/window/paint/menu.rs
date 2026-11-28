//! Контекстное меню, эллипсис и тесты.

use super::canvas::Canvas;
use crate::window::font::measure;
use light_vdom::layout::{FONT_SIZE, Rect};

use super::*;

pub(crate) fn draw_context_menu(
    canvas: &mut Canvas,
    menu: &super::super::state::ContextMenu,
    font: &crate::window::font::FontMetrics,
    scale: f32,
) {
    let w = MENU_W * scale;
    let row = MENU_ROW_H * scale;
    let border = MENU_BORDER * scale;
    let h = menu.items.len() as f32 * row + 2.0 * border;
    let rect = Rect::new(menu.x, menu.y, w, h);
    canvas.fill_rect(MENU_BG, rect);
    canvas.stroke_rect(MENU_BORDER_COLOR, rect, border);
    for (index, item) in menu.items.iter().enumerate() {
        let y = menu.y + border + index as f32 * row;
        if menu.hover == Some(index) && item.enabled {
            canvas.fill_rect(
                MENU_HOVER,
                Rect::new(menu.x + border, y, w - 2.0 * border, row),
            );
        }
        let color = if item.enabled {
            MENU_TEXT
        } else {
            MENU_TEXT_DISABLED
        };
        let size = FONT_SIZE * scale;
        let (_, label_h) = measure(font.font(), item.label, size);
        canvas.draw_text(
            font,
            item.label,
            menu.x + 10.0 * scale,
            y + (row - label_h) / 2.0,
            color,
            size,
        );
    }
}

/// Обрезает строку по ширине и добавляет `…`, если она не помещается.
pub(crate) fn ellipsized(
    font: &crate::window::font::FontMetrics,
    text: &str,
    size: f32,
    max_width: f32,
) -> String {
    let primary = font.font();
    if measure(primary, text, size).0 <= max_width {
        return text.to_owned();
    }
    let ellipsis = "\u{2026}";
    let ellipsis_width = measure(primary, ellipsis, size).0;
    if ellipsis_width > max_width {
        return ellipsis.to_owned();
    }
    let mut end = 0;
    let mut width = 0.0;
    for (index, ch) in text.char_indices() {
        let advance = crate::window::font::advance(primary, ch, size);
        if width + advance + ellipsis_width > max_width {
            break;
        }
        width += advance;
        end = index + ch.len_utf8();
    }
    let prefix = text[..end].trim_end();
    format!("{prefix}{ellipsis}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::window::font::measure;
    use crate::window::paint::forms::draw_focus_ring;

    pub(crate) fn pixel(pixels: &[u8], width: u32, x: u32, y: u32) -> u32 {
        let i = ((y * width + x) * 4) as usize;
        ((pixels[i] as u32) << 24)
            | ((pixels[i + 1] as u32) << 16)
            | ((pixels[i + 2] as u32) << 8)
            | pixels[i + 3] as u32
    }

    #[test]
    fn text_fits_vertical_line_box() {
        let Ok(metrics) = crate::window::font::FontMetrics::load() else {
            eprintln!(
                "SKIP: system font unavailable ({}); vertical line-box test not run",
                std::env::consts::OS
            );
            return;
        };
        let size = 20.0;
        let (_, height) = measure(metrics.font(), "Ag", size);
        let (width, canvas_height) = (80u32, 60u32);
        let mut pixels = vec![0u8; (width * canvas_height * 4) as usize];
        let region = Rect::new(0.0, 0.0, width as f32, canvas_height as f32);
        let mut canvas = Canvas::new(&mut pixels, width, canvas_height, region);
        let top = 10.0;
        canvas.draw_text(&metrics, "Ag", 5.0, top, 0xFFFF_FFFF, size);

        let mut min_y = canvas_height;
        let mut max_y = 0;
        for y in 0..canvas_height {
            for x in 0..width {
                if pixel(&pixels, width, x, y) != 0 {
                    min_y = min_y.min(y);
                    max_y = max_y.max(y);
                }
            }
        }

        assert!(min_y >= top as u32, "glyph above line box: {min_y}");
        assert!(
            max_y <= (top + height).ceil() as u32,
            "glyph below line box: {max_y} > {}",
            (top + height).ceil()
        );
    }

    #[test]
    fn ellipsized_text_fits_width() {
        let Ok(metrics) = crate::window::font::FontMetrics::load() else {
            eprintln!(
                "SKIP: system font unavailable ({}); ellipsis-width test not run",
                std::env::consts::OS
            );
            return;
        };
        let size = 16.0;
        let full = "very long label that must be truncated";
        let max = 90.0;
        let line = ellipsized(&metrics, full, size, max);

        assert!(line.ends_with('\u{2026}'), "добавлено многоточие");
        assert!(
            measure(metrics.font(), &line, size).0 <= max,
            "строка помещается"
        );
    }

    #[test]
    fn focus_ring_is_drawn_inset_inside_node() {
        let (width, height) = (24u32, 24u32);
        let mut pixels = vec![0u8; (width * height * 4) as usize];
        let region = Rect::new(0.0, 0.0, width as f32, height as f32);
        let mut canvas = Canvas::new(&mut pixels, width, height, region);
        draw_focus_ring(&mut canvas, Rect::new(4.0, 4.0, 16.0, 16.0), 1.0);

        // Кольцо с отступом 1px внутрь, толщиной 2px: рамка (5, 5, 14, 14).
        assert_eq!(pixel(&pixels, width, 5, 5), FOCUS_RING, "угол кольца");
        assert_eq!(pixel(&pixels, width, 6, 5), FOCUS_RING, "толщина 2px");
        assert_eq!(
            pixel(&pixels, width, 12, 12),
            0,
            "внутренность не закрашена"
        );
        assert_eq!(pixel(&pixels, width, 2, 2), 0, "снаружи узла не закрашено");
    }
}
