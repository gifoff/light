//! Векторный буфер отрисовки: примитивы, текст, изображения.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use ab_glyph::{Font, FontVec, PxScale, ScaleFont, point};

use light_vdom::layout::Rect;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct GlyphKey {
    font: usize,
    id: u16,
    size: u32,
    x: u32,
    baseline: u32,
}

#[derive(Debug)]
struct CachedGlyph {
    min_x: i32,
    min_y: i32,
    coverage: Vec<(u32, u32, f32)>,
}

thread_local! {
    /// Растрированные глифы: повторная отрисовка текста не запускает
    /// растеризацию заново (важно для смены вкладок и hover-состояний).
    static GLYPH_CACHE: RefCell<HashMap<GlyphKey, Rc<CachedGlyph>>> =
        RefCell::new(HashMap::new());
}

fn cached_glyph(
    font: &FontVec,
    id: ab_glyph::GlyphId,
    size: f32,
    pen: f32,
    baseline: f32,
) -> Option<Rc<CachedGlyph>> {
    let key = GlyphKey {
        font: font as *const FontVec as usize,
        id: id.0,
        size: size.to_bits(),
        x: pen.to_bits(),
        baseline: baseline.to_bits(),
    };
    if let Some(glyph) = GLYPH_CACHE.with(|cache| cache.borrow().get(&key).cloned()) {
        return Some(glyph);
    }
    let scale = PxScale::from(size);
    let glyph = id.with_scale_and_position(scale, point(pen, baseline));
    let outlined = font.outline_glyph(glyph)?;
    let bounds = outlined.px_bounds();
    let mut coverage = Vec::new();
    outlined.draw(|gx, gy, cov| {
        if cov > 0.0 {
            coverage.push((gx, gy, cov));
        }
    });
    let glyph = Rc::new(CachedGlyph {
        min_x: bounds.min.x as i32,
        min_y: bounds.min.y as i32,
        coverage,
    });
    GLYPH_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.len() < 8192 {
            cache.insert(key, Rc::clone(&glyph));
        }
    });
    Some(glyph)
}

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
        let row_start = (y0 as usize * self.width as usize + x0 as usize) * 4;
        let row_end = (y0 as usize * self.width as usize + x1 as usize) * 4;
        let stride = self.width as usize * 4;
        for y in y0 as usize..y1 as usize {
            let offset = (y - y0 as usize) * stride;
            for target in self.pixels[row_start + offset..row_end + offset]
                .as_chunks_mut::<4>()
                .0
            {
                target.copy_from_slice(&pixel);
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
        if color & 0xFF == 0xFF {
            let pixel = [
                (color >> 24) as u8,
                (color >> 16) as u8,
                (color >> 8) as u8,
                0xFF,
            ];
            let row_start = (y0 as u32 * self.width + x0 as u32) as usize * 4;
            let row_end = (y0 as u32 * self.width + x1 as u32) as usize * 4;
            for y in y0 as u32..y1 as u32 {
                let start = row_start + (y - y0 as u32) as usize * self.width as usize * 4;
                let end = row_end + (y - y0 as u32) as usize * self.width as usize * 4;
                for target in self.pixels[start..end].as_chunks_mut::<4>().0 {
                    target.copy_from_slice(&pixel);
                }
            }
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
    /// Высота строки — `ascent - descent` (см. [`crate::window::font::measure`]),
    /// поэтому базовая линия находится на `y + ascent`, а нижние выносные
    /// элементы доходят до `y + height`.
    ///
    /// Поддерживает фолбэк-шрифты для эмодзи: символ, отсутствующий в
    /// основном шрифте, ищется по цепочке фолбэков.
    pub fn draw_text(
        &mut self,
        metrics: &crate::window::font::FontMetrics,
        text: &str,
        x: f32,
        y: f32,
        color: u32,
        size: f32,
    ) {
        if text.is_empty() {
            return;
        }
        let primary = metrics.font();
        let scale_px = PxScale::from(size);
        let scaled = primary.as_scaled(scale_px);
        let baseline = y + scaled.ascent();
        let mut pen = x;
        for ch in text.chars() {
            match metrics.glyph_for(ch) {
                Some((font_idx, id)) => {
                    let font = &metrics.all_fonts()[font_idx];
                    if let Some(glyph) = cached_glyph(font, id, size, pen, baseline) {
                        for &(gx, gy, cov) in &glyph.coverage {
                            self.blend(
                                glyph.min_x + gx as i32,
                                glyph.min_y + gy as i32,
                                color,
                                cov,
                            );
                        }
                    }
                    let scaled_f = font.as_scaled(PxScale::from(size));
                    pen += scaled_f.h_advance(id);
                }
                None => {
                    // Плейсхолдер: рисуем □ (U+25A1) из основного шрифта.
                    let id = primary.glyph_id('\u{25A1}');
                    if let Some(glyph) = cached_glyph(primary, id, size, pen, baseline) {
                        for &(gx, gy, cov) in &glyph.coverage {
                            self.blend(
                                glyph.min_x + gx as i32,
                                glyph.min_y + gy as i32,
                                color,
                                cov,
                            );
                        }
                    }
                    pen += scaled.h_advance(id);
                }
            }
        }
    }

    /// Рисует изображение, растягивая его по nearest-neighbor в `rect`.
    ///
    /// Пиксели источника смешиваются с фоном как source-over с прямой альфой.
    #[cfg(feature = "image-support")]
    pub fn draw_image(&mut self, image: &super::super::image::DecodedImage, rect: Rect) {
        if image.width == 0 || image.height == 0 || rect.w <= 0.0 || rect.h <= 0.0 {
            return;
        }
        let (cx0, cy0, cx1, cy1) = self.clip;
        let x0 = rect.x.floor().max(cx0 as f32) as i32;
        let y0 = rect.y.floor().max(cy0 as f32) as i32;
        let x1 = (rect.x + rect.w).ceil().min(cx1 as f32) as i32;
        let y1 = (rect.y + rect.h).ceil().min(cy1 as f32) as i32;
        for y in y0..y1 {
            let v = ((y as f32 + 0.5 - rect.y) / rect.h * image.height as f32) as u32;
            let v = v.min(image.height - 1);
            for x in x0..x1 {
                let u = ((x as f32 + 0.5 - rect.x) / rect.w * image.width as f32) as u32;
                let u = u.min(image.width - 1);
                let index = ((v * image.width + u) * 4) as usize;
                let pixel = &image.pixels[index..index + 4];
                let color = (pixel[0] as u32) << 24
                    | (pixel[1] as u32) << 16
                    | (pixel[2] as u32) << 8
                    | pixel[3] as u32;
                self.blend(x, y, color, 1.0);
            }
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
