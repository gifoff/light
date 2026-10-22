//! Программная отрисовка: прямоугольники, рамки и текст в пиксельный буфер
//! формата RGBA.

use ab_glyph::{Font, FontVec, PxScale, ScaleFont, point};

use super::font::measure;
#[cfg(feature = "elements-select")]
use crate::layout::SELECT_PAD_X;
#[cfg(any(
    feature = "elements-checkbox",
    feature = "elements-radio",
    feature = "elements-select"
))]
use crate::layout::{CHECKBOX_BOX, CHECKBOX_GAP};
use crate::layout::{FIELD_PAD_X, FONT_SIZE, Layout, PlacedNode, Rect};

/// Цвет в формате `0xRRGGBBAA`.
pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> u32 {
    ((r as u32) << 24) | ((g as u32) << 16) | ((b as u32) << 8) | a as u32
}

/// Непрозрачный цвет в формате `0xRRGGBBAA`.
pub const fn rgb(r: u8, g: u8, b: u8) -> u32 {
    rgba(r, g, b, 0xFF)
}

/// Основной текст.
pub const COLOR_TEXT: u32 = rgb(0x1F, 0x24, 0x2B);
/// Приглушённый текст (placeholder).
pub const COLOR_MUTED: u32 = rgb(0x8A, 0x92, 0x9E);
/// Фон титульной полосы.
pub const TITLEBAR_BG: u32 = rgb(0xDF, 0xE4, 0xEB);
/// Нижняя граница титульной полосы.
pub const TITLEBAR_BORDER: u32 = rgb(0xC4, 0xCB, 0xD6);
/// Заливка кнопки.
pub const BUTTON_BG: u32 = rgb(0xE4, 0xE8, 0xEF);
/// Заливка кнопки под курсором.
pub const BUTTON_BG_HOVER: u32 = rgb(0xD3, 0xDA, 0xE4);
/// Заливка заблокированной кнопки.
pub const BUTTON_BG_DISABLED: u32 = rgb(0xEE, 0xF0, 0xF3);
/// Рамка кнопки.
pub const BUTTON_BORDER: u32 = rgb(0xC2, 0xC9, 0xD4);
/// Подпись заблокированной кнопки.
pub const BUTTON_LABEL_DISABLED: u32 = rgb(0xA6, 0xAD, 0xB8);
/// Заливка текстового поля.
pub const FIELD_BG: u32 = rgb(0xFF, 0xFF, 0xFF);
/// Рамка текстового поля.
pub const FIELD_BORDER: u32 = rgb(0xB8, 0xC0, 0xCC);
/// Рамка текстового поля с фокусом.
pub const FIELD_BORDER_FOCUS: u32 = rgb(0x3B, 0x82, 0xF6);
/// Каретка ввода.
pub const COLOR_CARET: u32 = rgb(0x1F, 0x24, 0x2B);
/// Подсветка выделенного текста.
pub const SELECTION_BG: u32 = rgb(0xB6, 0xD4, 0xFE);
/// Индикатор изменения размера поля.
pub const RESIZE_GRIP: u32 = rgb(0x9A, 0xA2, 0xAE);
/// Заливка флажка/переключателя.
#[cfg(any(feature = "elements-checkbox", feature = "elements-radio"))]
pub const FORM_BG: u32 = rgb(0xFF, 0xFF, 0xFF);
/// Акцент флажка/переключателя/списка.
#[cfg(any(
    feature = "elements-checkbox",
    feature = "elements-radio",
    feature = "elements-select"
))]
pub const FORM_ACCENT: u32 = rgb(0x3B, 0x82, 0xF6);
/// Рамка поля, не прошедшего валидацию.
pub const FIELD_INVALID: u32 = rgb(0xD9, 0x53, 0x4F);
/// Цвет индикатора обязательного поля (`*`).
pub const REQUIRED_COLOR: u32 = rgb(0xD9, 0x53, 0x4F);
/// Цвет кольца фокуса клавиатурной навигации.
pub const FOCUS_RING: u32 = rgb(0x4A, 0x90, 0xD9);
/// Толщина кольца фокуса (px).
pub const FOCUS_RING_WIDTH: f32 = 2.0;
/// Отступ кольца фокуса внутрь узла (px).
pub const FOCUS_RING_OFFSET: f32 = 1.0;

/// Ширина контекстного меню.
pub(crate) const MENU_W: f32 = 160.0;
/// Высота строки контекстного меню.
pub(crate) const MENU_ROW_H: f32 = 26.0;
/// Толщина рамки контекстного меню.
pub(crate) const MENU_BORDER: f32 = 1.0;
/// Фон контекстного меню.
const MENU_BG: u32 = rgb(0xFF, 0xFF, 0xFF);
/// Рамка контекстного меню.
const MENU_BORDER_COLOR: u32 = rgb(0xC4, 0xCB, 0xD6);
/// Подсветка пункта под курсором.
const MENU_HOVER: u32 = rgb(0xE3, 0xEC, 0xFB);
/// Доступный пункт меню.
const MENU_TEXT: u32 = rgb(0x1F, 0x24, 0x2B);
/// Заблокированный пункт меню.
const MENU_TEXT_DISABLED: u32 = rgb(0xA6, 0xAD, 0xB8);

/// Пиксельный буфер кадра (RGBA, 4 байта на пиксель) с ограничением (clip)
/// на повреждённую область.
///
/// Всё, что рисуется через [`Canvas`], попадает только внутрь clip — это и
/// есть точечная перерисовка: каждый повреждённый прямоугольник окрашивается
/// заново, остальные пиксели остаются прежними.
pub struct Canvas<'a> {
    pixels: &'a mut [u8],
    width: u32,
    /// Границы области: `(x0, y0, x1, y1)` — полуинтервал в пикселях.
    clip: (u32, u32, u32, u32),
}

impl<'a> Canvas<'a> {
    /// Создаёт холст над буфером, ограниченный прямоугольником `region`
    /// (координаты окна, выход за границы буфера отсекается).
    pub fn new(pixels: &'a mut [u8], width: u32, height: u32, region: Rect) -> Self {
        let x0 = region.x.floor().max(0.0);
        let y0 = region.y.floor().max(0.0);
        let x1 = (region.x + region.w).ceil().min(width as f32);
        let y1 = (region.y + region.h).ceil().min(height as f32);
        Self {
            pixels,
            width,
            clip: (
                x0.max(0.0) as u32,
                y0.max(0.0) as u32,
                x1.max(x0.max(0.0)) as u32,
                y1.max(y0.max(0.0)) as u32,
            ),
        }
    }

    /// Пересекается ли прямоугольник с областью холста.
    pub fn intersects(&self, rect: Rect) -> bool {
        let (x0, y0, x1, y1) = self.clip;
        let rx0 = rect.x.max(x0 as f32);
        let ry0 = rect.y.max(y0 as f32);
        let rx1 = (rect.x + rect.w).min(x1 as f32);
        let ry1 = (rect.y + rect.h).min(y1 as f32);
        rx0 < rx1 && ry0 < ry1
    }

    /// Выполняет рисование `draw` с дополнительной обрезкой по `rect`.
    ///
    /// Всё, что рисуется внутри, не выходит за пределы `rect` — так содержимое
    /// текстового поля скрывается за его границами.
    pub fn clipped(&mut self, rect: Rect, draw: impl FnOnce(&mut Self)) {
        let saved = self.clip;
        let x0 = (rect.x.floor().max(0.0) as u32).max(self.clip.0);
        let y0 = (rect.y.floor().max(0.0) as u32).max(self.clip.1);
        let x1 = ((rect.x + rect.w).ceil().max(0.0) as u32).min(self.clip.2);
        let y1 = ((rect.y + rect.h).ceil().max(0.0) as u32).min(self.clip.3);
        self.clip = (x0, y0, x1.max(x0), y1.max(y0));
        draw(self);
        self.clip = saved;
    }

    /// Заливает всю область холста цветом фона (замена пикселей, а не
    /// смешивание — область перерисовывается с нуля).
    pub fn clear(&mut self, color: u32) {
        let (x0, y0, x1, y1) = self.clip;
        let pixel = [
            (color >> 24) as u8,
            (color >> 16) as u8,
            (color >> 8) as u8,
            color as u8,
        ];
        for y in y0..y1 {
            for x in x0..x1 {
                self.set(x, y, pixel);
            }
        }
    }

    /// Заливает прямоугольник (с обрезкой по области холста).
    pub fn fill_rect(&mut self, color: u32, rect: Rect) {
        let (cx0, cy0, cx1, cy1) = self.clip;
        let x0 = rect.x.floor().max(cx0 as f32);
        let y0 = rect.y.floor().max(cy0 as f32);
        let x1 = (rect.x + rect.w).ceil().min(cx1 as f32);
        let y1 = (rect.y + rect.h).ceil().min(cy1 as f32);
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        for y in y0 as u32..y1 as u32 {
            for x in x0 as u32..x1 as u32 {
                self.blend(x as i32, y as i32, color, 1.0);
            }
        }
    }

    /// Рисует рамку толщиной `thickness` по краям прямоугольника.
    pub fn stroke_rect(&mut self, color: u32, rect: Rect, thickness: f32) {
        self.fill_rect(color, Rect::new(rect.x, rect.y, rect.w, thickness));
        self.fill_rect(
            color,
            Rect::new(rect.x, rect.y + rect.h - thickness, rect.w, thickness),
        );
        self.fill_rect(color, Rect::new(rect.x, rect.y, thickness, rect.h));
        self.fill_rect(
            color,
            Rect::new(rect.x + rect.w - thickness, rect.y, thickness, rect.h),
        );
    }

    /// Заливает круг (с обрезкой по области холста).
    #[cfg(any(
        feature = "elements-checkbox",
        feature = "elements-radio",
        feature = "elements-select"
    ))]
    pub fn fill_circle(&mut self, color: u32, cx: f32, cy: f32, radius: f32) {
        if radius <= 0.0 {
            return;
        }
        let (cx0, cy0, cx1, cy1) = self.clip;
        let x0 = (cx - radius).floor().max(cx0 as f32) as i32;
        let y0 = (cy - radius).floor().max(cy0 as f32) as i32;
        let x1 = (cx + radius).ceil().min(cx1 as f32) as i32;
        let y1 = (cy + radius).ceil().min(cy1 as f32) as i32;
        let r2 = radius * radius;
        for y in y0..y1 {
            for x in x0..x1 {
                let dx = x as f32 + 0.5 - cx;
                let dy = y as f32 + 0.5 - cy;
                if dx * dx + dy * dy <= r2 {
                    self.blend(x, y, color, 1.0);
                }
            }
        }
    }

    /// Рисует отрезок толщиной `thickness` (с обрезкой по области холста).
    #[cfg(any(
        feature = "elements-checkbox",
        feature = "elements-radio",
        feature = "elements-select"
    ))]
    pub fn draw_line(&mut self, color: u32, from: (f32, f32), to: (f32, f32), thickness: f32) {
        let (x0, y0) = from;
        let (x1, y1) = to;
        let dx = x1 - x0;
        let dy = y1 - y0;
        let length = (dx * dx + dy * dy).sqrt();
        let steps = length.ceil().max(1.0) as i32;
        let radius = (thickness / 2.0).max(0.5);
        for step in 0..=steps {
            let t = step as f32 / steps as f32;
            self.fill_circle(color, x0 + dx * t, y0 + dy * t, radius);
        }
    }

    /// Рисует строку: `(x, y)` — левый верхний угол типографской строки.
    ///
    /// Высота строки — `ascent - descent` (см. [`super::font::measure`]),
    /// поэтому базовая линия находится на `y + ascent`, а нижние выносные
    /// элементы доходят до `y + height`.
    pub fn draw_text(&mut self, font: &FontVec, text: &str, x: f32, y: f32, color: u32, size: f32) {
        if text.is_empty() {
            return;
        }
        let scale_px = PxScale::from(size);
        let scaled = font.as_scaled(scale_px);
        let baseline = y + scaled.ascent();
        let mut pen = x;
        for ch in text.chars() {
            let id = font.glyph_id(ch);
            let glyph = id.with_scale_and_position(scale_px, point(pen, baseline));
            if let Some(outlined) = font.outline_glyph(glyph) {
                let bounds = outlined.px_bounds();
                outlined.draw(|gx, gy, cov| {
                    self.blend(
                        bounds.min.x as i32 + gx as i32,
                        bounds.min.y as i32 + gy as i32,
                        color,
                        cov,
                    );
                });
            }
            pen += scaled.h_advance(id);
        }
    }

    fn set(&mut self, x: u32, y: u32, pixel: [u8; 4]) {
        let index = (y * self.width + x) as usize * 4;
        if let Some(target) = self.pixels.get_mut(index..index + 4) {
            target.copy_from_slice(&pixel);
        }
    }

    /// Смешивает цвет (source-over, прямая альфа) с покрытием глифа
    /// (`cov` от 0 до 1).
    fn blend(&mut self, x: i32, y: i32, color: u32, cov: f32) {
        let coverage = cov.clamp(0.0, 1.0);
        if coverage <= 0.0 || x < 0 || y < 0 {
            return;
        }
        let (cx0, cy0, cx1, cy1) = (
            self.clip.0 as i32,
            self.clip.1 as i32,
            self.clip.2 as i32,
            self.clip.3 as i32,
        );
        if x < cx0 || x >= cx1 || y < cy0 || y >= cy1 {
            return;
        }
        let index = (y as u32 * self.width + x as u32) as usize * 4;
        let Some(target) = self.pixels.get_mut(index..index + 4) else {
            return;
        };
        // source-over с прямой (не premultiplied) альфой.
        let src_a = ((color & 0xFF) as f32 / 255.0) * coverage;
        if src_a <= 0.0 {
            return;
        }
        let dst_a = target[3] as f32 / 255.0;
        let out_a = src_a + dst_a * (1.0 - src_a);
        if out_a <= 0.0 {
            target.copy_from_slice(&[0, 0, 0, 0]);
            return;
        }
        let channel = |shift: u32, dst: u8| {
            let src = ((color >> shift) & 0xFF) as f32 / 255.0;
            let dst = dst as f32 / 255.0;
            let out = (src * src_a + dst * dst_a * (1.0 - src_a)) / out_a;
            (out * 255.0).round().clamp(0.0, 255.0) as u8
        };
        let (dr, dg, db) = (target[0], target[1], target[2]);
        target[0] = channel(24, dr);
        target[1] = channel(16, dg);
        target[2] = channel(8, db);
        target[3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
    }
}

/// Состояние взаимодействия для отрисовки: поле в фокусе, позиция каретки,
/// выделение и кнопка под курсором.
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
fn mark_disabled_text(nodes: &[PlacedNode], index: usize, inherited: bool, out: &mut [bool]) {
    let disabled = inherited
        || matches!(
            nodes[index].kind,
            crate::vdom::NodeKind::Button { disabled: true, .. }
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
pub fn render(
    layout: &Layout,
    canvas: &mut Canvas,
    font: &FontVec,
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
            crate::vdom::NodeKind::TitleBar => Some(style.background.unwrap_or(TITLEBAR_BG)),
            crate::vdom::NodeKind::Button { disabled, .. } => {
                Some(style.background.unwrap_or(if *disabled {
                    BUTTON_BG_DISABLED
                } else if hover == Some(node.key) {
                    BUTTON_BG_HOVER
                } else {
                    BUTTON_BG
                }))
            }
            crate::vdom::NodeKind::TextField { .. } => Some(style.background.unwrap_or(FIELD_BG)),
            #[cfg(feature = "elements-select")]
            crate::vdom::NodeKind::Select { .. } => Some(style.background.unwrap_or(FIELD_BG)),
            _ => style.background,
        };
        if let Some(color) = background {
            canvas.fill_rect(color, rect);
        }
        match &node.kind {
            crate::vdom::NodeKind::Column
            | crate::vdom::NodeKind::Row
            | crate::vdom::NodeKind::Window => {}
            crate::vdom::NodeKind::TitleBar => {
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
            crate::vdom::NodeKind::Text { content } => {
                let color = if style.font_color.is_none() && dimmed[index] {
                    BUTTON_LABEL_DISABLED
                } else {
                    style.font_color.unwrap_or(COLOR_TEXT)
                };
                canvas.draw_text(
                    font,
                    content,
                    rect.x + inset.left,
                    rect.y + inset.top,
                    color,
                    size,
                );
            }
            crate::vdom::NodeKind::Button { .. } => {
                canvas.stroke_rect(style.border_color.unwrap_or(BUTTON_BORDER), rect, thickness);
            }
            crate::vdom::NodeKind::TextField {
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
                if *resize != crate::vdom::Resize::None {
                    draw_grip(canvas, rect, scale);
                }
                if style.required {
                    draw_field_required(canvas, font, rect, scale, size, style.required_indicator);
                }
            }
            #[cfg(feature = "elements-checkbox")]
            crate::vdom::NodeKind::Checkbox {
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
                        + measure(font, label, size).0;
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
            crate::vdom::NodeKind::Radio {
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
                        + measure(font, label, size).0;
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
            crate::vdom::NodeKind::Select {
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
                    draw_field_required(canvas, font, rect, scale, size, style.required_indicator);
                }
            }
        }
    }

    // Кольцо фокуса рисуется поверх дерева, вокруг узла с клавиатурным
    // фокусом. Текстовые поля имеют собственную рамку фокуса.
    if let Some(key) = focus_ring
        && let Some(node) = layout.node(key)
        && !matches!(node.kind, crate::vdom::NodeKind::TextField { .. })
    {
        draw_focus_ring(canvas, node.rect, scale);
    }

    #[cfg(feature = "elements-select")]
    if let Some(key) = open_select
        && let Some(node) = layout.node(key)
        && let crate::vdom::NodeKind::Select { value, options, .. } = &node.kind
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

/// Рисует кольцо клавиатурного фокуса внутри прямоугольника узла.
fn draw_focus_ring(canvas: &mut Canvas, rect: Rect, scale: f32) {
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
fn draw_field_required(
    canvas: &mut Canvas,
    font: &FontVec,
    rect: Rect,
    scale: f32,
    size: f32,
    indicator: crate::vdom::RequiredIndicator,
) {
    match indicator {
        crate::vdom::RequiredIndicator::None => {}
        crate::vdom::RequiredIndicator::Asterisk => {
            let (width, _) = measure(font, "*", size);
            canvas.draw_text(
                font,
                "*",
                rect.x + rect.w - width - 4.0 * scale,
                rect.y + 1.0 * scale,
                REQUIRED_COLOR,
                size,
            );
        }
        crate::vdom::RequiredIndicator::Accent(color) => {
            let width = (2.0 * scale).max(2.0);
            canvas.fill_rect(color, Rect::new(rect.x, rect.y, width, rect.h));
        }
    }
}

/// Рисует индикатор обязательности флажка/переключателя после подписи.
#[cfg(any(feature = "elements-checkbox", feature = "elements-radio"))]
fn draw_inline_required(
    canvas: &mut Canvas,
    font: &FontVec,
    text_end_x: f32,
    rect: Rect,
    scale: f32,
    size: f32,
    indicator: crate::vdom::RequiredIndicator,
) {
    match indicator {
        crate::vdom::RequiredIndicator::None => {}
        crate::vdom::RequiredIndicator::Asterisk => {
            let (_, text_h) = measure(font, "*", size);
            canvas.draw_text(
                font,
                "*",
                text_end_x + 4.0 * scale,
                rect.y + (rect.h - text_h) / 2.0,
                REQUIRED_COLOR,
                size,
            );
        }
        crate::vdom::RequiredIndicator::Accent(color) => {
            let width = (2.0 * scale).max(2.0);
            canvas.fill_rect(color, Rect::new(rect.x, rect.y, width, rect.h));
        }
    }
}

/// Рисует флажок с подписью.
#[allow(clippy::too_many_arguments)]
#[cfg(feature = "elements-checkbox")]
fn draw_checkbox(
    canvas: &mut Canvas,
    font: &FontVec,
    rect: Rect,
    inset: crate::vdom::Edges,
    scale: f32,
    size: f32,
    label: &str,
    checked: bool,
    disabled: bool,
    style: crate::vdom::Style,
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
    let (_, text_h) = measure(font, label, size);
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
fn draw_radio(
    canvas: &mut Canvas,
    font: &FontVec,
    rect: Rect,
    inset: crate::vdom::Edges,
    scale: f32,
    size: f32,
    label: &str,
    selected: bool,
    disabled: bool,
    style: crate::vdom::Style,
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
    let (_, text_h) = measure(font, label, size);
    canvas.draw_text(
        font,
        label,
        left + box_size + gap,
        rect.y + (rect.h - text_h) / 2.0,
        color,
        size,
    );
}

/// Рисует выпадающий список (без раскрытого меню).
#[allow(clippy::too_many_arguments)]
#[cfg(feature = "elements-select")]
fn draw_select(
    canvas: &mut Canvas,
    font: &FontVec,
    rect: Rect,
    inset: crate::vdom::Edges,
    scale: f32,
    size: f32,
    thickness: f32,
    value: &str,
    options: &[crate::vdom::SelectOption],
    disabled: bool,
    style: crate::vdom::Style,
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
    let (_, text_h) = measure(font, label, size);
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
fn draw_select_popup(
    canvas: &mut Canvas,
    font: &FontVec,
    rect: Rect,
    options: &[crate::vdom::SelectOption],
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
        let (_, text_h) = measure(font, &option.label, size);
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
fn render_field(
    canvas: &mut Canvas,
    font: &FontVec,
    rect: Rect,
    inset: crate::vdom::Edges,
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
    let row_h = super::text::row_height(font, size);
    let line_box_h = measure(font, "", size).1;
    let inner = Rect::new(
        rect.x + border,
        rect.y + border,
        (rect.w - 2.0 * border).max(0.0),
        (rect.h - 2.0 * border).max(0.0),
    );
    let lines = super::text::layout(font, text, size, text_max_w, multiline);

    if multiline {
        let content = Rect::new(
            rect.x + inset.left,
            rect.y + inset.top,
            (rect.w - inset.horizontal()).max(0.0),
            (rect.h - inset.vertical()).max(0.0),
        );
        let visible = (content.h / row_h).floor().max(1.0);
        let caret_line = super::text::line_of(&lines, caret_pos);
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
            let px = super::text::prefix_width(font, &line.text, size, offset);
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
            super::text::prefix_width(font, text, size, caret_pos.min(text.chars().count()))
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
fn draw_selection(
    canvas: &mut Canvas,
    font: &FontVec,
    line: &super::text::Line,
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
    let x0 = text_left + super::text::prefix_width(font, &line.text, size, from - line.start);
    let x1 = text_left + super::text::prefix_width(font, &line.text, size, to - line.start);
    canvas.fill_rect(SELECTION_BG, Rect::new(x0, y, x1 - x0, height));
}

/// Рисует индикатор изменения размера в правом нижнем углу поля.
fn draw_grip(canvas: &mut Canvas, rect: Rect, scale: f32) {
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

/// Рисует контекстное меню поверх остального интерфейса.
pub(super) fn draw_context_menu(
    canvas: &mut Canvas,
    menu: &super::ContextMenu,
    font: &FontVec,
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
        let (_, label_h) = measure(font, item.label, size);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(pixels: &[u8], width: u32, x: u32, y: u32) -> u32 {
        let i = ((y * width + x) * 4) as usize;
        ((pixels[i] as u32) << 24)
            | ((pixels[i + 1] as u32) << 16)
            | ((pixels[i + 2] as u32) << 8)
            | pixels[i + 3] as u32
    }

    #[test]
    fn text_fits_vertical_line_box() {
        let Ok(metrics) = crate::window::font::FontMetrics::load() else {
            return;
        };
        let font = metrics.font();
        let size = 20.0;
        let (_, height) = measure(font, "Ag", size);
        let (width, canvas_height) = (80u32, 60u32);
        let mut pixels = vec![0u8; (width * canvas_height * 4) as usize];
        let region = Rect::new(0.0, 0.0, width as f32, canvas_height as f32);
        let mut canvas = Canvas::new(&mut pixels, width, canvas_height, region);
        let top = 10.0;
        canvas.draw_text(font, "Ag", 5.0, top, 0xFFFF_FFFF, size);

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
