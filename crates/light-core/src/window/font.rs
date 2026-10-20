//! Загрузка системного шрифта и измерение текста.

use std::path::PathBuf;

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
}

impl FontMetrics {
    /// Загружает системный шрифт.
    pub fn load() -> Result<Self, Error> {
        Ok(Self {
            font: load_system_font()?,
        })
    }

    /// Шрифт для отрисовки.
    pub fn font(&self) -> &FontVec {
        &self.font
    }
}

impl TextMetrics for FontMetrics {
    fn measure(&self, text: &str, font_size: f32) -> (f32, f32) {
        measure(&self.font, text, font_size)
    }
}

/// Размер строки при заданном размере шрифта: (ширина, высота).
pub fn measure(font: &FontVec, text: &str, font_size: f32) -> (f32, f32) {
    let scaled = font.as_scaled(PxScale::from(font_size));
    let width: f32 = text
        .chars()
        .map(|ch| scaled.h_advance(font.glyph_id(ch)))
        .sum();
    (width, scaled.height())
}

/// Горизонтальный шаг одного символа при заданном размере шрифта.
pub fn advance(font: &FontVec, ch: char, font_size: f32) -> f32 {
    let scaled = font.as_scaled(PxScale::from(font_size));
    scaled.h_advance(font.glyph_id(ch))
}

/// Высота строки шрифта при заданном размере.
pub fn line_height(font: &FontVec, font_size: f32) -> f32 {
    font.as_scaled(PxScale::from(font_size)).height()
}
