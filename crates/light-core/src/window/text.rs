//! Разбиение текста текстового поля на визуальные строки и отображение
//! позиции каретки. Используется и отрисовкой, и обработкой ввода.

use ab_glyph::FontVec;

use super::font::{advance, line_height};

/// Одна визуальная строка текста.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    /// Индекс первого символа строки в исходном тексте (в символах).
    pub start: usize,
    /// Текст строки (без завершающего перевода строки).
    pub text: String,
    /// Ширина строки в пикселях.
    pub width: f32,
}

/// Высота строки шрифта размером `size`.
pub fn row_height(font: &FontVec, size: f32) -> f32 {
    line_height(font, size)
}

/// Разбивает `text` на визуальные строки.
///
/// При `multiline` включается перенос длинных строк по ширине `max_width` и
/// разделение по `\n`; иначе всё содержимое — одна строка.
pub fn layout(font: &FontVec, text: &str, size: f32, max_width: f32, multiline: bool) -> Vec<Line> {
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut current_w = 0.0_f32;
    let mut start = 0usize;
    let mut index = 0usize;
    for ch in text.chars() {
        if ch == '\n' && multiline {
            lines.push(Line {
                start,
                text: std::mem::take(&mut current),
                width: current_w,
            });
            index += 1;
            start = index;
            current_w = 0.0;
            continue;
        }
        let w = advance(font, ch, size);
        if multiline && !current.is_empty() && current_w + w > max_width {
            lines.push(Line {
                start,
                text: std::mem::take(&mut current),
                width: current_w,
            });
            start = index;
            current_w = 0.0;
        }
        current.push(ch);
        current_w += w;
        index += 1;
    }
    lines.push(Line {
        start,
        text: current,
        width: current_w,
    });
    lines
}

/// Ширина первых `count` символов строки `text`.
pub fn prefix_width(font: &FontVec, text: &str, size: f32, count: usize) -> f32 {
    text.chars()
        .take(count)
        .map(|ch| advance(font, ch, size))
        .sum()
}

/// Индекс визуальной строки, содержащей символ `caret`.
///
/// Если `lines` пуст, возвращает `0`.
pub fn line_of(lines: &[Line], caret: usize) -> usize {
    let mut result = 0;
    for (index, line) in lines.iter().enumerate() {
        if line.start <= caret {
            result = index;
        } else {
            break;
        }
    }
    result
}
