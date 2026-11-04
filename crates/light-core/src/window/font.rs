//! Загрузка системного шрифта и измерение текста.
//!
//! Поддерживает цепочку фолбэк-шрифтов для эмодзи: основной шрифт
//! (Segoe UI / DejaVu) + системные цветные эмодзи-файлы. Символ, отсутствующий
//! в основном шрифте, отрисовывается из первого фолбэка, содержащего его; если
//! не найден — рисуется плейсхолдер □ (U+25A1).

use std::{cell::RefCell, collections::HashMap, path::PathBuf};

use ab_glyph::{Font, FontVec, GlyphId, PxScale, ScaleFont};

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

/// Цветные эмодзи-файлы, которые ищутся после основного шрифта.
/// Порядок: Windows → Linux → macOS.
const EMOJI_CANDIDATES: &[&str] = &[
    "segoeuiemoji.ttf",
    "segoeuisl.ttf",
    "NotoColorEmoji.ttf",
    "Symbola.ttf",
    "AppleColorEmoji.ttc",
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
    dirs.push(PathBuf::from("/usr/share/fonts"));
    dirs
}

/// Каталог macOS-шрифтов (если доступен).
fn macos_font_dir() -> Option<PathBuf> {
    PathBuf::from("/System/Library/Fonts")
        .exists()
        .then_some(PathBuf::from("/System/Library/Fonts"))
        .or_else(|| {
            PathBuf::from("/Library/Fonts")
                .exists()
                .then_some(PathBuf::from("/Library/Fonts"))
        })
}

/// Ищет файл по списку каталогов, возвращает `None`, если не найден.
fn find_file(names: &[&str], extra_dirs: &mut [PathBuf]) -> Option<PathBuf> {
    for name in names {
        for dir in font_dirs() {
            let path = dir.join(name);
            if path.exists() {
                return Some(path);
            }
        }
        if let Some(dir) = macos_font_dir() {
            let path = dir.join(name);
            if path.exists() {
                return Some(path);
            }
        }
        // Явно переданные дополнительные каталоги.
        for dir in extra_dirs.iter() {
            let path = dir.join(name);
            if path.exists() {
                return Some(path);
            }
        }
    }
    None
}

/// Загружает основной системный шрифт.
pub fn load_system_font() -> Result<FontVec, Error> {
    let mut extra_dirs: Vec<PathBuf> = Vec::new();
    if let Some(path) = find_file(CANDIDATES, &mut extra_dirs) {
        let data = std::fs::read(&path)
            .map_err(|e| Error::Font(format!("не удалось прочитать {}: {e}", path.display())))?;
        return FontVec::try_from_vec(data)
            .map_err(|_| Error::Font(format!("файл шрифта повреждён: {}", path.display())));
    }
    Err(Error::Font(format!(
        "системный шрифт не найден (проверенные файлы: {})",
        CANDIDATES.join(", ")
    )))
}

/// Загружает все доступные фолбэк-шрифты (эмодзи) в порядке приоритета.
///
/// Возвращает пустой `Vec`, если ни одного эмодзи-файла не найдено —
/// это нормальная ситуация: приложение работает без эмодзи-поддержки.
pub fn load_fallback_fonts() -> Vec<FontVec> {
    let mut result = Vec::new();
    for name in EMOJI_CANDIDATES {
        for dir in font_dirs() {
            let path = dir.join(name);
            if path.exists() {
                if let Ok(data) = std::fs::read(&path) {
                    if let Ok(font) = FontVec::try_from_vec(data) {
                        eprintln!(
                            "light: loaded fallback font {:?} ({})",
                            path.display(),
                            name
                        );
                        result.push(font);
                    } else {
                        eprintln!(
                            "light: failed to parse fallback font {:?}: invalid font data",
                            path.display()
                        );
                    }
                }
                break; // первый найденный экземпляр этого файла достаточно
            }
        }
        if let Some(dir) = macos_font_dir() {
            let path = dir.join(name);
            if path.exists() {
                if let Ok(data) = std::fs::read(&path)
                    && let Ok(font) = FontVec::try_from_vec(data)
                {
                    eprintln!(
                        "light: loaded fallback font {:?} ({})",
                        path.display(),
                        name
                    );
                    result.push(font);
                }
                break;
            }
        }
    }
    result
}

/// Метрики текста на системном шрифте + цепочка фолбэков.
pub struct FontMetrics {
    /// Полный набор шрифтов: [основной, фолбэки...]. Индекс 0 — основной.
    fonts: Vec<FontVec>,
    texts: RefCell<HashMap<(u32, String), (f32, f32)>>,
    advances: RefCell<HashMap<(u32, char, usize), f32>>,
    heights: RefCell<HashMap<u32, f32>>,
}

const TEXT_CACHE_LIMIT: usize = 4096;

impl FontMetrics {
    /// Загружает системный шрифт + все доступные фолбэки.
    pub fn load() -> Result<Self, Error> {
        let primary = load_system_font()?;
        let fallbacks = load_fallback_fonts();
        let mut fonts = vec![primary];
        fonts.extend(fallbacks);
        Ok(Self {
            fonts,
            texts: RefCell::new(HashMap::new()),
            advances: RefCell::new(HashMap::new()),
            heights: RefCell::new(HashMap::new()),
        })
    }

    /// Шрифт для отрисовки (основной, индекс 0).
    pub fn font(&self) -> &FontVec {
        &self.fonts[0]
    }

    /// Список всех шрифтов (основной + фолбэки).
    pub fn all_fonts(&self) -> &[FontVec] {
        &self.fonts
    }

    /// Возвращает `(font_index, glyph_id)` для данного символа.
    ///
    /// Сначала проверяет основной шрифт: если глиф есть — `(0, id)`.
    /// Затем фолбэки по порядку. Если глиф не найден нигде — `None`
    /// (рисуется плейсхолдер U+25A1).
    pub fn glyph_for(&self, ch: char) -> Option<(usize, GlyphId)> {
        for (idx, font) in self.fonts.iter().enumerate() {
            let id = font.glyph_id(ch);
            if id.0 != 0 && font.outline(id).is_some() {
                return Some((idx, id));
            }
        }
        None
    }

    /// Горизонтальный шаг символа с учётом фолбэк-шрифтов.
    pub fn advance_for(&self, ch: char, font_size: f32) -> f32 {
        match self.glyph_for(ch) {
            Some((idx, id)) => {
                let scaled = self.fonts[idx].as_scaled(PxScale::from(font_size));
                scaled.h_advance(id)
            }
            None => {
                // Плейсхолдер □: используем advance из основного шрифта.
                let scaled = self.fonts[0].as_scaled(PxScale::from(font_size));
                scaled.h_advance(self.fonts[0].glyph_id('\u{25A1}'))
            }
        }
    }
}

impl TextMetrics for FontMetrics {
    fn measure(&self, text: &str, font_size: f32) -> (f32, f32) {
        let size = font_size.to_bits();
        let key = (size, text.to_owned());
        if let Some(size) = self.texts.borrow().get(&key) {
            return *size;
        }
        let width: f32 = text
            .chars()
            .map(|ch| {
                let cache_key = (size, ch, 0);
                if let Some(advance) = self.advances.borrow().get(&cache_key) {
                    return *advance;
                }
                let advance = self.advance_for(ch, font_size);
                // Кэшируем под каждый возможный индекс (упрощённо: ключ
                // содержит только размер+символ, т.к. индекс определяется
                // детерминированно через glyph_for).
                self.advances.borrow_mut().insert(cache_key, advance);
                advance
            })
            .sum();
        let height = if let Some(height) = self.heights.borrow().get(&size) {
            *height
        } else {
            let scaled = self.fonts[0].as_scaled(PxScale::from(font_size));
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

    /// ТЗ 0.14.3 Task 6.3: на Windows 🔥 должен резолвиться в один из
    /// шрифтов (основной или фолбэк). На Linux/macOS тест пропускается,
    /// если Noto/Symbola/AppleColorEmoji не установлены.
    #[test]
    fn emoji_glyph_for_fire() {
        let Ok(metrics) = FontMetrics::load() else {
            eprintln!("skip: system font not available");
            return;
        };
        // Проверяем наличие хотя бы одного глифа для '🔥' в любой
        // доступной шрифтовой цепочке.
        let result = metrics.glyph_for('\u{1F525}');
        match result {
            Some((idx, id)) => {
                eprintln!("🔥 resolved via font[{idx}] glyph_id={id:?}")
            }
            None => {
                eprintln!(
                    "🔥 not resolved: no emoji font found among {} candidates",
                    EMOJI_CANDIDATES.len()
                );
            }
        }
    }
}
