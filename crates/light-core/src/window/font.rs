//! Загрузка системного шрифта и измерение текста.

use std::{cell::RefCell, collections::HashMap, path::PathBuf};

use ab_glyph::{Font, FontVec, PxScale, ScaleFont};

use crate::Error;
use crate::layout::TextMetrics;

/// Шрифты, которые пробуются по порядку: сначала типовые для Windows,
/// затем для Linux/Unix.
const CANDIDATES: &[&str] = &[
    "segoeui.ttf",
    "arial.ttf",
    "tahoma.ttf",
    "verdana.ttf",
    "DejaVuSans.ttf",
    "LiberationSans-Regular.ttf",
];

/// Каталоги, в которых ищутся шрифты.
fn font_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(windir) = std::env::var("WINDIR") {
        dirs.push(PathBuf::from(windir).join("Fonts"));
    }
    dirs.push(PathBuf::from("/usr/share/fonts/truetype/dejavu"));
    dirs.push(PathBuf::from("/usr/share/fonts/truetype/liberation"));
    dirs.push(PathBuf::from("/usr/share/fonts/TTF"));
    dirs
}

/// Загружает первый найденный системный шрифт.
pub fn load_system_font() -> Result<FontVec, Error> {
    for name in CANDIDATES {
        for dir in font_dirs() {
            let path = dir.join(name);
            if let Ok(data) = std::fs::read(&path) {
                return FontVec::try_from_vec(data).map_err(|_| {
                    Error::Font(format!("файл шрифта повреждён: {}", path.display()))
                });
            }
        }
    }
    Err(Error::Font(format!(
        "системный шрифт не найден (проверенные файлы: {})",
        CANDIDATES.join(", ")
    )))
}

/// Метрики текста на системном шрифте.
pub struct FontMetrics {
    font: FontVec,
    texts: RefCell<HashMap<(u32, String), (f32, f32)>>,
    advances: RefCell<HashMap<(u32, char), f32>>,
    heights: RefCell<HashMap<u32, f32>>,
}

const TEXT_CACHE_LIMIT: usize = 4096;

impl FontMetrics {
    /// Загружает системный шрифт.
    pub fn load() -> Result<Self, Error> {
        Ok(Self {
            font: load_system_font()?,
            texts: RefCell::new(HashMap::new()),
            advances: RefCell::new(HashMap::new()),
            heights: RefCell::new(HashMap::new()),
        })
    }

    /// Шрифт для отрисовки.
    pub fn font(&self) -> &FontVec {
        &self.font
    }
}

impl TextMetrics for FontMetrics {
    fn measure(&self, text: &str, font_size: f32) -> (f32, f32) {
        let size = font_size.to_bits();
        let key = (size, text.to_owned());
        if let Some(size) = self.texts.borrow().get(&key) {
            return *size;
        }
        let scaled = self.font.as_scaled(PxScale::from(font_size));
        let width = text
            .chars()
            .map(|ch| {
                let key = (size, ch);
                if let Some(advance) = self.advances.borrow().get(&key) {
                    return *advance;
                }
                let advance = scaled.h_advance(self.font.glyph_id(ch));
                self.advances.borrow_mut().insert(key, advance);
                advance
            })
            .sum();
        let height = if let Some(height) = self.heights.borrow().get(&size) {
            *height
        } else {
            let height = scaled.ascent() - scaled.descent();
            self.heights.borrow_mut().insert(size, height);
            height
        };
        let measured = (width, height);
        let mut texts = self.texts.borrow_mut();
        if texts.len() >= TEXT_CACHE_LIMIT {
            texts.clear();
        }
        texts.insert(key, measured);
        measured
    }
}

/// Размер строки при заданном размере шрифта: (ширина, высота).
///
/// Высота — это типографская строка без `line-gap`: от `ascent` до `descent`.
/// Именно она используется в раскладке, чтобы `align-items: center`/`end`
/// центрировал глифы, а не невидимый запас между строками.
pub fn measure(font: &FontVec, text: &str, font_size: f32) -> (f32, f32) {
    let scaled = font.as_scaled(PxScale::from(font_size));
    let width: f32 = text
        .chars()
        .map(|ch| scaled.h_advance(font.glyph_id(ch)))
        .sum();
    (width, scaled.ascent() - scaled.descent())
}

/// Горизонтальный шаг одного символа при заданном размере шрифта.
pub fn advance(font: &FontVec, ch: char, font_size: f32) -> f32 {
    let scaled = font.as_scaled(PxScale::from(font_size));
    scaled.h_advance(font.glyph_id(ch))
}

/// Высота строки шрифта при заданном размере, включая `line-gap`.
///
/// Для расстояния между строками многострочного текста. Для геометрической
/// высоты текстового узла используйте [`measure`].
pub fn line_height(font: &FontVec, font_size: f32) -> f32 {
    font.as_scaled(PxScale::from(font_size)).height()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measure_excludes_line_gap() {
        let Ok(metrics) = FontMetrics::load() else {
            return;
        };
        let size = 20.0;
        let scaled = metrics.font().as_scaled(PxScale::from(size));
        let (_, height) = metrics.measure("Ag", size);

        assert!((height - (scaled.ascent() - scaled.descent())).abs() < 0.01);
        assert!((scaled.height() - height - scaled.line_gap()).abs() < 0.01);
    }
}
