use crate::{
    component::{Component, WindowConfig},
    vdom::{
        Align, ButtonType, Edges, Offset, Overflow, Position, RequiredIndicator, Resize, Size,
        Style, parse_align_items, parse_bool, parse_color, parse_justify, parse_offset,
        parse_overflow, parse_position, parse_required_indicator, parse_resize, parse_size_or_auto,
        parse_white_space,
    },
};

use super::parse::parse_fail;

/// Внутренний: парсинг стиля из атрибутов элемента.
pub(crate) fn build_style<C: Component + ?Sized>(
    tag: &str,
    attrs: &[light_fw_html::Attr],
    default_font: Option<f32>,
    _component: &C,
) -> Option<Style> {
    let mut width = Size::Auto;
    let mut height = Size::Auto;
    let mut min_width = None;
    let mut min_height = None;
    let mut max_width = None;
    let mut max_height = None;
    let mut flex_grow = 0.0f32;
    let mut flex_shrink = 1.0f32;
    let mut position = Position::Flow;
    let mut overflow = Overflow::Visible;
    let mut overflow_x = None;
    let mut overflow_y = None;
    let mut white_space = None;
    let mut align_x = Align::Start;
    let mut align_y = Align::Start;
    let mut offset_x = Offset::Px(0.0);
    let mut offset_y = Offset::Px(0.0);
    let mut justify_content = Align::Start;
    let mut align_items = Align::Start;
    let mut padding = Edges::ZERO;
    let mut margin = Edges::ZERO;
    let mut background = None;
    let mut font_color = None;
    let mut border_color = None;
    let mut font_size = default_font;
    let mut border_size = 0.0f32;
    let mut visible = true;
    let mut required = false;
    let mut required_indicator = RequiredIndicator::Asterisk;
    let mut z = 0i32;

    for attr in attrs {
        let name = attr.name.as_str();
        let value = attr.value.as_str();
        // Логирует `parse_fail` при `None` и прокидывает значение дальше.
        // Четыре монотипных замыкания — по одному на каждый тип значения
        // (`f32`, `Size`, `u32`, `i32`), так как Rust не разрешает генерики
        // в замыкании без явной аннотации, а явная аннотация делает вызов
        // громоздким (см. замечание о generic-inference trap).
        let strict = |v: Option<f32>| -> Option<f32> {
            if v.is_none() {
                parse_fail(tag, name, value);
            }
            v
        };
        let strict_size = |n: &str, v: &str, s: Option<Size>| -> Option<Size> {
            if s.is_none() {
                parse_fail(tag, n, v);
            }
            s
        };
        let strict_color = |n: &str, v: &str, c: Option<u32>| -> Option<u32> {
            if c.is_none() {
                parse_fail(tag, n, v);
            }
            c
        };
        let strict_i32 = |n: &str, v: &str, i: Option<i32>| -> Option<i32> {
            if i.is_none() {
                parse_fail(tag, n, v);
            }
            i
        };
        match name {
            "width" => width = strict_size(name, value, parse_size(value))?,
            "height" => height = strict_size(name, value, parse_size(value))?,
            "min-width" => min_width = strict_size(name, value, parse_size(value)),
            "max-width" => max_width = strict_size(name, value, parse_size(value)),
            "min-height" => min_height = strict_size(name, value, parse_size(value)),
            "max-height" => max_height = strict_size(name, value, parse_size(value)),
            "flex-grow" => flex_grow = strict(parse_flex(value))?,
            "flex-shrink" => flex_shrink = strict(parse_flex(value))?,
            "flex" => {
                let parts: Vec<&str> = value.split_whitespace().collect();
                if !parts.is_empty() {
                    flex_grow = strict(parse_flex(parts[0]))?;
                }
                if parts.len() > 1 {
                    flex_shrink = strict(parse_flex(parts[1]))?;
                }
            }
            "position" => position = parse_position(value),
            "overflow" => overflow = parse_overflow(value),
            "overflow-x" => overflow_x = Some(parse_overflow(value)),
            "overflow-y" => overflow_y = Some(parse_overflow(value)),
            "white-space" => white_space = Some(parse_white_space(value)),
            // `left`/`center`/`right` (и `top`/`center`/`bottom`) принимаются
            // в любом регистре; любое другое значение не является ошибкой —
            // родитель выравнивает его через `align-items`.
            "position-x" => {
                let t = value.trim().to_ascii_lowercase();
                align_x = match t.as_str() {
                    "left" => Align::Start,
                    "center" => Align::Center,
                    "right" => Align::End,
                    _ => Align::Start,
                }
            }
            "position-y" => {
                let t = value.trim().to_ascii_lowercase();
                align_y = match t.as_str() {
                    "top" => Align::Start,
                    "center" => Align::Center,
                    "bottom" => Align::End,
                    _ => Align::Start,
                }
            }
            "position-offset-x" => offset_x = parse_offset(value),
            "position-offset-y" => offset_y = parse_offset(value),
            "justify-content" => justify_content = parse_justify(value),
            "align-items" => align_items = parse_align_items(value),
            "padding" => set_edges_all(&mut padding, strict(parse_px_strict(value))?),
            "margin" => set_edges_all(&mut margin, strict(parse_px_strict(value))?),
            "padding-top" | "padding-right" | "padding-bottom" | "padding-left" => {
                set_edge_side(&mut padding, name, strict(parse_px_strict(value))?)
            }
            "margin-top" | "margin-right" | "margin-bottom" | "margin-left" => {
                set_edge_side(&mut margin, name, strict(parse_px_strict(value))?)
            }
            "border-size" => border_size = strict(parse_px_strict(value))?,
            "font-size" => font_size = strict(parse_px_strict(value)),
            "background" => background = strict_color(name, value, parse_color(value)),
            "font-color" => font_color = strict_color(name, value, parse_color(value)),
            "border-color" => border_color = strict_color(name, value, parse_color(value)),
            "visible" => visible = !matches!(value, "false" | "0" | "no"),
            "required" => required = !matches!(value, "false" | "0" | "no"),
            "required-indicator" => required_indicator = parse_required_indicator(value),
            "z" => z = strict_i32(name, value, value.trim().parse::<i32>().ok()).unwrap_or(0),
            other if other.starts_with(':') => {
                parse_fail(tag, other, value);
                return None;
            }
            _ => {}
        }
    }
    let ox = overflow_x.unwrap_or(overflow);
    let oy = overflow_y.unwrap_or(overflow);
    Some(Style {
        width,
        height,
        min_width,
        min_height,
        max_width,
        max_height,
        flex_grow,
        flex_shrink,
        position,
        overflow_x: ox,
        overflow_y: oy,
        white_space,
        align_x,
        align_y,
        offset_x,
        offset_y,
        justify_content,
        align_items,
        padding,
        margin,
        background,
        font_color,
        border_color,
        font_size,
        border_size,
        visible,
        required,
        invalid: false,
        required_indicator,
        z,
    })
}

fn parse_size(value: &str) -> Option<Size> {
    Some(parse_size_or_auto(value))
}

fn parse_flex(value: &str) -> Option<f32> {
    let n: f32 = value.trim().parse().ok()?;
    if !n.is_finite() || n < 0.0 {
        return None;
    }
    Some(n)
}

/// Строго разбирает значение в пикселях (`12`, `12px`): нечисло — ошибка
/// (возвращает `None`). Пустое значение даёт `0.0`.
fn parse_px_strict(value: &str) -> Option<f32> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Some(0.0);
    }
    let number = trimmed.strip_suffix("px").unwrap_or(trimmed).trim();
    number.parse::<f32>().ok()
}

fn set_edges_all(edges: &mut Edges, px: f32) {
    edges.top = px;
    edges.right = px;
    edges.bottom = px;
    edges.left = px;
}

fn set_edge_side(edges: &mut Edges, name: &str, px: f32) {
    match name {
        "padding-top" | "margin-top" => edges.top = px,
        "padding-right" | "margin-right" => edges.right = px,
        "padding-bottom" | "margin-bottom" => edges.bottom = px,
        "padding-left" | "margin-left" => edges.left = px,
        _ => {}
    }
}

/// Строго разбирает атрибут `resize`: неизвестное значение — ошибка (возвращает
/// `None`). Пустое и `"none"` дают [`Resize::None`].
/// Внутренний: парсинг значения resize.
pub(crate) fn parse_resize_value(value: &str) -> Option<Resize> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("none") {
        return Some(Resize::None);
    }
    Some(parse_resize(trimmed))
}

// ---------------------------------------------------------------------------
// Вспомогательные функции
// ---------------------------------------------------------------------------

/// Значение первого атрибута с именем `name`.
pub(crate) fn attr_value<'a>(attrs: &'a [light_fw_html::Attr], name: &str) -> Option<&'a str> {
    attrs
        .iter()
        .find(|a| a.name == name)
        .map(|a| a.value.as_str())
}

/// Является ли имя атрибута конструкцией, не поддерживаемой во время выполнения
/// (директива `v-*` или обработчик, кроме простых `v-if`, `v-model` и `@click`).
///
/// `v-if` поддерживается в runtime с 0.21.0 (см. [`eval_v_if`](super::parse::eval_v_if)):
/// она эвалюируется **до** попадания сюда, поэтому здесь тоже исключена.
pub(crate) fn is_unsupported_directive(name: &str) -> bool {
    (name.starts_with("v-")
        && name != "v-model"
        && name != "v-if")
        || (name.starts_with('@') && name != "@click")
}

/// Текстовое содержимое элемента (все текстовые дети склеены).
///
/// Возвращает `None`, если в подписи встречается интерполяция `{{ … }}`:
/// динамические подписи во время выполнения не поддерживаются.
#[cfg_attr(
    all(not(feature = "elements-checkbox"), not(feature = "elements-radio")),
    allow(dead_code)
)]
pub(crate) fn text_content(children: &[light_fw_html::Node]) -> Option<String> {
    let mut out = String::new();
    for child in children {
        if let light_fw_html::Node::Text(segments) = child {
            for segment in segments {
                match segment {
                    light_fw_html::Segment::Literal(text) => out.push_str(text),
                    light_fw_html::Segment::Path(_path) => {
                        return None;
                    }
                }
            }
        }
    }
    Some(out)
}

/// Запрашивает строковое значение поля через [`Component::eval_field`];
/// простой идентификатор (без точек) обязателен.
pub(crate) fn eval_simple<C: Component + ?Sized>(component: &C, name: &str) -> Option<String> {
    if name.contains('.') || name.is_empty() {
        return None;
    }
    match component.eval_field(name) {
        Some(v) => Some(v),
        None => {
            // Метод (а не поле): возвращаем пустую строку, чтобы runtime-рендер
            // продолжал работу. Статическое дерево будет содержать правильное
            // значение при следующей пересборке.
            Some(String::new())
        }
    }
}

/// Запрашивает булево значение поля: `"true"/"1"/"yes"/"on"` → `true`.
pub(crate) fn eval_bool<C: Component + ?Sized>(component: &C, name: &str) -> Option<bool> {
    let raw = eval_simple(component, name)?;
    Some(parse_bool(&raw))
}

/// Разрешает атрибут `:disabled`/`disabled`: динамический — через
/// [`Component::eval_field`], статический — по булеву флагу.
pub(crate) fn resolve_disabled<C: Component + ?Sized>(
    attrs: &[light_fw_html::Attr],
    component: &C,
) -> Option<bool> {
    if let Some(v) = attr_value(attrs, ":disabled") {
        return eval_bool(component, v.trim());
    }
    attr_value(attrs, "disabled")
        .map(|v| !matches!(v, "false" | "0" | "no"))
        .or(Some(false))
}

/// Разрешает атрибут `type` кнопки: `:type` — через [`Component::eval_field`],
/// статический `type` — по имени.
pub(crate) fn resolve_button_type<C: Component + ?Sized>(
    attrs: &[light_fw_html::Attr],
    component: &C,
) -> Option<ButtonType> {
    if let Some(v) = attr_value(attrs, ":type") {
        let name = v.trim();
        let field = eval_simple(component, name)?;
        return button_type_from_name(&field);
    }
    let name = attr_value(attrs, "type").unwrap_or("");
    button_type_from_name(name)
}

fn button_type_from_name(name: &str) -> Option<ButtonType> {
    let trimmed = name.trim().to_ascii_lowercase();
    match trimmed.as_str() {
        "" | "submit" => Some(ButtonType::Submit),
        "button" => Some(ButtonType::Button),
        "reset" => Some(ButtonType::Reset),
        _ => None,
    }
}

/// Собирает статические (не-директивные) атрибуты и тег для `with_tag`/`with_attrs`.
/// Внутренний: собирает метаданные (сырой тег и атрибуты).
pub(crate) fn collect_meta(
    attrs: &[light_fw_html::Attr],
    tag: &str,
) -> (String, Vec<(String, String)>) {
    let mut out = Vec::new();
    for attr in attrs {
        let name = attr.name.as_str();
        if name.starts_with('@')
            || name.starts_with("v-")
            || name.starts_with(':')
            || matches!(name, "key" | "required-indicator" | "src" | "validator")
        {
            continue;
        }
        out.push((name.to_string(), attr.value.clone()));
    }
    (tag.to_string(), out)
}

/// Строит [`WindowConfig`] из атрибутов корневого `<window>`.
///
/// Иконка не поддерживается во время выполнения (требует `&'static [u8]`),
/// поэтому она игнорируется.
/// Внутренний: конфигурация окна из атрибутов элемента.
pub(crate) fn build_window_config(element: &light_fw_html::Element) -> WindowConfig {
    let mut cfg = WindowConfig::default();
    let get = |name: &str| attr_value(&element.attrs, name).map(str::to_string);
    if let Some(v) = get("width") {
        cfg.width = v.parse().unwrap_or(cfg.width);
    }
    if let Some(v) = get("height") {
        cfg.height = v.parse().unwrap_or(cfg.height);
    }
    if let Some(v) = get("min-width") {
        cfg.min_width = Some(v.parse().unwrap_or(cfg.width));
    }
    if let Some(v) = get("min-height") {
        cfg.min_height = Some(v.parse().unwrap_or(cfg.height));
    }
    if let Some(v) = get("max-width") {
        cfg.max_width = Some(v.parse().unwrap_or(cfg.width));
    }
    if let Some(v) = get("max-height") {
        cfg.max_height = Some(v.parse().unwrap_or(cfg.height));
    }
    if let Some(v) = get("title") {
        cfg.title = v.trim().to_string();
    }
    if let Some(v) = get("modal") {
        cfg.modal = if v.trim().is_empty() {
            true
        } else {
            parse_bool(&v)
        };
    }
    if let Some(v) = get("visible") {
        cfg.visible = if v.trim().is_empty() {
            true
        } else {
            parse_bool(&v)
        };
    }
    if let Some(v) = get("background")
        && let Some(color) = parse_color(&v)
    {
        cfg.background = color;
    }
    if let Some(v) = get("transparent") {
        cfg.transparent = if v.trim().is_empty() {
            true
        } else {
            parse_bool(&v)
        };
    }
    if let Some(v) = get("resizable") {
        cfg.resizable = parse_bool(&v);
    }
    if let Some(v) = get("id") {
        let id = v.trim().to_string();
        if !id.is_empty() {
            cfg.id = Some(id);
        }
    }
    if let Some(v) = get("parent") {
        let parent = v.trim().to_string();
        if !parent.is_empty() {
            cfg.parent = Some(parent);
        }
    }
    cfg
}
