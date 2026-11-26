//! Общие хелперы для виджетов light-ui (0.21.0).
//!
//! Единая копия [`parse_color_named`] (ранее была продублирована в 4 файлах)
//! и [`parse_z`] для политики z-слоёв: дефолт применяется **только** если
//! атрибут `z` отсутствует; явный `z` переопределяет.

use std::str::FromStr;

use light_fw_core::vdom::parse::parse_color;

/// Разбирает цвет: CSS-имена (через lower-case) + общие VDOM-форматы
/// `#RRGGBB[AA]`/`0x…` (порядок байт — **`0xRRGGBBAA`**, т.е. alpha в
/// младшем байте; совпадает с VDOM-тестами: `#FF0000` → `0xFF0000FF`).
pub(crate) fn parse_color_named(value: &str) -> Option<u32> {
    match value.trim().to_ascii_lowercase().as_str() {
        "black" => Some(0x00_00_00_FF),
        "white" => Some(0xFF_FF_FF_FF),
        "red" => Some(0xFF_00_00_FF),
        "green" => Some(0x00_FF_00_FF),
        "blue" => Some(0x00_00_FF_FF),
        "yellow" => Some(0xFF_FF_00_FF),
        "orange" => Some(0xFF_7F_00_FF),
        "purple" => Some(0x80_00_80_FF),
        "gray" | "grey" => Some(0x80_80_80_FF),
        _ => parse_color(value),
    }
}

/// Разбирает целочисленное значение z-слоя (`"500"`, `"1000"`).
/// Возвращает `None` для нечисла или пустой строки.
pub(crate) fn parse_z(value: &str) -> Option<i32> {
    i32::from_str(value.trim()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Валидное значение разбирается (с пробелами по краям).
    #[test]
    fn parse_z_valid() {
        assert_eq!(parse_z("500"), Some(500));
        assert_eq!(parse_z("  1000  "), Some(1000));
        assert_eq!(parse_z("0"), Some(0));
    }

    /// Мусор (не число) → `None`.
    #[test]
    fn parse_z_junk() {
        assert_eq!(parse_z("abc"), None);
        assert_eq!(parse_z("5px"), None);
        assert_eq!(parse_z("1.5"), None);
    }

    /// Пустая строка и только пробелы → `None`.
    #[test]
    fn parse_z_empty() {
        assert_eq!(parse_z(""), None);
        assert_eq!(parse_z("   "), None);
    }

    /// Отрицательное значение разбирается корректно.
    #[test]
    fn parse_z_negative() {
        assert_eq!(parse_z("-50"), Some(-50));
        assert_eq!(parse_z(" -7 "), Some(-7));
    }
}
