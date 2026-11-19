use super::events::{
    Align, ButtonType, Offset, Overflow, Position, RequiredIndicator, Resize, Size, WhiteSpace,
};

fn size_or_auto_impl(value: &str) -> Option<Size> {
    let value = value.trim();
    if value.is_empty() || value.eq_ignore_ascii_case("auto") {
        return Some(Size::Auto);
    }
    if let Some(percent) = value.strip_suffix('%') {
        return percent.trim().parse::<f32>().ok().map(Size::Percent);
    }
    let number = value.strip_suffix("px").unwrap_or(value).trim();
    number.parse::<f32>().ok().map(Size::Px)
}

/// Разбирает значение размера (`width`/`height`) во время выполнения.
///
/// Правила совпадают со временем компиляции шаблонов: `auto`/пусто —
/// [`Size::Auto`], `50%` — [`Size::Percent`], `120` или `120px` — [`Size::Px`].
/// Некорректное значение в debug-сборке вызывает `debug_assert!`, а в release
/// возвращает [`Size::Auto`].
pub fn parse_size_or_auto(value: &str) -> Size {
    size_or_auto_impl(value).unwrap_or_else(|| {
        debug_assert!(false, "invalid size value: \"{value}\"");
        Size::Auto
    })
}

fn offset_impl(value: &str) -> Option<Offset> {
    let value = value.trim();
    if let Some(percent) = value.strip_suffix('%') {
        return percent.trim().parse::<f32>().ok().map(Offset::Percent);
    }
    let number = value.strip_suffix("px").unwrap_or(value).trim();
    number.parse::<f32>().ok().map(Offset::Px)
}

/// Разбирает значение смещения (`position-offset-x`/`position-offset-y`) во
/// время выполнения: `-8` и `12px` — [`Offset::Px`], `10%` — [`Offset::Percent`]
/// (знак допускается). Некорректное значение в debug-сборке вызывает
/// `debug_assert!`, а в release возвращает [`Offset::default`].
pub fn parse_offset(value: &str) -> Offset {
    offset_impl(value).unwrap_or_else(|| {
        debug_assert!(false, "invalid offset value: \"{value}\"");
        Offset::default()
    })
}

fn align_impl(value: &str, start: &str, end: &str) -> Option<Align> {
    match value.trim().to_ascii_lowercase().as_str() {
        value if value == start => Some(Align::Start),
        "center" => Some(Align::Center),
        value if value == end => Some(Align::End),
        _ => None,
    }
}

/// Разбирает `position-x` (`left`/`center`/`right`). Некорректное значение в
/// debug-сборке вызывает `debug_assert!`, а в release возвращает
/// [`Align::Start`].
pub fn parse_align_x(value: &str) -> Align {
    align_impl(value, "left", "right").unwrap_or_else(|| {
        debug_assert!(false, "invalid position-x value: \"{value}\"");
        Align::Start
    })
}

/// Разбирает `position-y` (`top`/`center`/`bottom`). Некорректное значение в
/// debug-сборке вызывает `debug_assert!`, а в release возвращает
/// [`Align::Start`].
pub fn parse_align_y(value: &str) -> Align {
    align_impl(value, "top", "bottom").unwrap_or_else(|| {
        debug_assert!(false, "invalid position-y value: \"{value}\"");
        Align::Start
    })
}

/// Разбирает `justify-content` (`start`/`center`/`end`). Некорректное значение в
/// debug-сборке вызывает `debug_assert!`, а в release возвращает
/// [`Align::Start`].
pub fn parse_justify(value: &str) -> Align {
    align_impl(value, "start", "end").unwrap_or_else(|| {
        debug_assert!(false, "invalid justify-content value: \"{value}\"");
        Align::Start
    })
}

/// Разбирает `align-items` (`start`/`center`/`end`). Некорректное значение
/// не прерывает работу приложения: печатается warning в stderr и
/// возвращается [`Align::Start`].
pub fn parse_align_items(value: &str) -> Align {
    match align_impl(value, "start", "end") {
        Some(align) => align,
        None => {
            eprintln!("warning: invalid align-items value: {value:?}, using 'start'");
            Align::Start
        }
    }
}

fn overflow_impl(value: &str) -> Option<Overflow> {
    match value.trim().to_ascii_lowercase().as_str() {
        "visible" => Some(Overflow::Visible),
        "hidden" => Some(Overflow::Hidden),
        "auto" => Some(Overflow::Auto),
        "scroll" => Some(Overflow::Scroll),
        _ => None,
    }
}

/// Разбирает `overflow`/`overflow-x`/`overflow-y` (`visible`, `hidden`,
/// `auto`, `scroll`). Некорректное значение в debug-сборке вызывает
/// `debug_assert!`, а в release возвращает [`Overflow::Visible`].
pub fn parse_overflow(value: &str) -> Overflow {
    overflow_impl(value).unwrap_or_else(|| {
        debug_assert!(false, "invalid overflow value: \"{value}\"");
        Overflow::Visible
    })
}

fn white_space_impl(value: &str) -> Option<WhiteSpace> {
    match value.trim().to_ascii_lowercase().as_str() {
        "normal" | "wrap" => Some(WhiteSpace::Normal),
        "nowrap" | "no-wrap" => Some(WhiteSpace::Nowrap),
        _ => None,
    }
}

/// Разбирает `white-space` (`normal`/`wrap` или `nowrap`). Некорректное
/// значение в debug-сборке вызывает `debug_assert!`, а в release возвращает
/// [`WhiteSpace::Normal`].
pub fn parse_white_space(value: &str) -> WhiteSpace {
    white_space_impl(value).unwrap_or_else(|| {
        debug_assert!(false, "invalid white-space value: \"{value}\"");
        WhiteSpace::Normal
    })
}

fn position_impl(value: &str) -> Option<Position> {
    match value.trim().to_ascii_lowercase().as_str() {
        "flow" | "static" | "relative" => Some(Position::Flow),
        "absolute" => Some(Position::Absolute),
        _ => None,
    }
}

/// Разбирает `position` (`flow`/`static`/`relative` или `absolute`).
/// Некорректное значение в debug-сборке вызывает `debug_assert!`, а в release
/// возвращает [`Position::Flow`].
pub fn parse_position(value: &str) -> Position {
    position_impl(value).unwrap_or_else(|| {
        debug_assert!(false, "invalid position value: \"{value}\"");
        Position::Flow
    })
}

fn px_impl(value: &str) -> Option<f32> {
    let value = value.trim();
    let number = value.strip_suffix("px").unwrap_or(value).trim();
    number.parse::<f32>().ok()
}

/// Разбирает значение в пикселях (`12`, `12px`). Некорректное значение в
/// debug-сборке вызывает `debug_assert!`, а в release возвращает `0.0`.
pub fn parse_px(value: &str) -> f32 {
    px_impl(value).unwrap_or_else(|| {
        debug_assert!(false, "invalid pixel value: \"{value}\"");
        0.0
    })
}

fn color_impl(value: &str) -> Option<u32> {
    let raw = value.trim();
    let digits = raw
        .strip_prefix('#')
        .or_else(|| raw.strip_prefix("0x"))
        .unwrap_or(raw);
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |text: &str| u8::from_str_radix(text, 16).ok();
    let (r, g, b, a) = match digits.len() {
        3 => (
            byte(&digits[0..1])? * 17,
            byte(&digits[1..2])? * 17,
            byte(&digits[2..3])? * 17,
            0xFF,
        ),
        4 => (
            byte(&digits[0..1])? * 17,
            byte(&digits[1..2])? * 17,
            byte(&digits[2..3])? * 17,
            byte(&digits[3..4])? * 17,
        ),
        6 => (
            byte(&digits[0..2])?,
            byte(&digits[2..4])?,
            byte(&digits[4..6])?,
            0xFF,
        ),
        8 => (
            byte(&digits[0..2])?,
            byte(&digits[2..4])?,
            byte(&digits[4..6])?,
            byte(&digits[6..8])?,
        ),
        _ => return None,
    };
    Some(((r as u32) << 24) | ((g as u32) << 16) | ((b as u32) << 8) | a as u32)
}

/// Разбирает цвет (`#RGB`, `#RGBA`, `#RRGGBB`, `#RRGGBBAA`, `0xRRGGBB`,
/// `0xRRGGBBAA`) в `0xRRGGBBAA`. Возвращает `None` для некорректного значения
/// (в debug-сборке дополнительно вызывает `debug_assert!`).
pub fn parse_color(value: &str) -> Option<u32> {
    let parsed = color_impl(value);
    debug_assert!(
        parsed.is_some(),
        "invalid color value: \"{value}\" (#RGB, #RGBA, #RRGGBB, #RRGGBBAA)"
    );
    parsed
}

/// Разбирает булев атрибут: истина для `true`/`1`/`yes`/`on`.
pub fn parse_bool(value: &str) -> bool {
    matches!(value.trim(), "true" | "1" | "yes" | "on")
}

fn required_indicator_impl(value: &str) -> Option<RequiredIndicator> {
    match value.trim() {
        "" | "*" => Some(RequiredIndicator::Asterisk),
        "none" => Some(RequiredIndicator::None),
        other => color_impl(other).map(RequiredIndicator::Accent),
    }
}

/// Разбирает `required-indicator`: `none`, `*` (по умолчанию) или цвет
/// (`#RRGGBB`). Некорректное значение в debug-сборке вызывает `debug_assert!`,
/// а в release возвращает [`RequiredIndicator::Asterisk`].
pub fn parse_required_indicator(value: &str) -> RequiredIndicator {
    required_indicator_impl(value).unwrap_or_else(|| {
        debug_assert!(false, "invalid required-indicator value: \"{value}\"");
        RequiredIndicator::Asterisk
    })
}

fn button_type_impl(value: &str) -> Option<ButtonType> {
    match value.trim().to_ascii_lowercase().as_str() {
        "" | "submit" => Some(ButtonType::Submit),
        "button" => Some(ButtonType::Button),
        "reset" => Some(ButtonType::Reset),
        _ => None,
    }
}

/// Разбирает тип кнопки (`submit`/`button`/`reset`). Некорректное значение в
/// debug-сборке вызывает `debug_assert!`, а в release возвращает
/// [`ButtonType::Submit`].
pub fn parse_button_type(value: &str) -> ButtonType {
    button_type_impl(value).unwrap_or_else(|| {
        debug_assert!(false, "invalid button type value: \"{value}\"");
        ButtonType::Submit
    })
}

fn resize_impl(value: &str) -> Option<Resize> {
    match value.trim().to_ascii_lowercase().as_str() {
        "" | "none" => Some(Resize::None),
        "horizontal" => Some(Resize::Horizontal),
        "vertical" => Some(Resize::Vertical),
        "both" => Some(Resize::Both),
        _ => None,
    }
}

/// Разбирает режим изменения размера поля
/// (`none`/`horizontal`/`vertical`/`both`). Некорректное значение в debug-сборке
/// вызывает `debug_assert!`, а в release возвращает [`Resize::None`].
pub fn parse_resize(value: &str) -> Resize {
    resize_impl(value).unwrap_or_else(|| {
        debug_assert!(false, "invalid resize value: \"{value}\"");
        Resize::None
    })
}
