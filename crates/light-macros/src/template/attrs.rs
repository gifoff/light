//! Р РµР°Р»РёР·Р°С†РёСЏ Р°С‚СЂРёР±СѓС‚Р° `#[template]`: С‡С‚РµРЅРёРµ HTML-С„Р°Р№Р»Р° Рё СЂР°Р·СЂРµС€РµРЅРёРµ
//! `<include>` РЅР° СЌС‚Р°РїРµ Р С”Р С•Р СР С—Р С‘Р В»РЎРЏРЎвЂ Р С‘Р С‘, РіРµРЅРµСЂР°С†РёСЏ СЂРµР°Р»РёР·Р°С†РёРё `Component`.

use proc_macro2::{Span, TokenStream};
use quote::quote;

use super::parser::Generator;
use super::tokens::{
    color_attr, edges_attr, px_attr, required_attr, required_indicator_attr, white_space_attr,
};
use super::validation::attr_value;
use light_fw_html::Element;

pub(super) fn escape_format(literal: &str) -> String {
    literal.replace('{', "{{").replace('}', "}}")
}

/// Разбирает атрибут размера (`width`/`height`): число в пикселях
/// (`120`, `120px`) или проценты от родителя (`50%`).
pub(super) fn parse_size_attr(element: &Element, name: &str) -> syn::Result<Option<TokenStream>> {
    let Some(value) = attr_value(&element.attrs, name) else {
        return Ok(None);
    };
    let value = value.trim();
    let invalid = || {
        syn::Error::new(
            Span::call_site(),
            format!(
                "invalid {name}=\"{value}\" on <{}>: expected pixels (120) or percent (50%)",
                element.name
            ),
        )
    };
    let token = if let Some(percent) = value.strip_suffix('%') {
        let number: f32 = percent.trim().parse().map_err(|_| invalid())?;
        quote!(::light_fw::vdom::Size::Percent(#number))
    } else {
        let number = value.strip_suffix("px").unwrap_or(value).trim();
        let number: f32 = number.parse().map_err(|_| invalid())?;
        quote!(::light_fw::vdom::Size::Px(#number))
    };
    Ok(Some(token))
}

/// Собирает [`light_fw::vdom::Style`] из атрибутов: `width`, `height` (px или %),
/// `position-x` (`left`/`center`/`right`), `position-y` (`top`/`center`/
/// `bottom`), `position-offset-x`/`position-offset-y` (px или %, в том числе
/// отрицательные; псевдонимы `left`/`top`), `padding`/`margin` и их
/// построчные варианты `padding-top`/`padding-right`/`padding-bottom`/
/// `padding-left` (аналогично `margin-*`), `font-size`,
/// `border-size` (px), `background`, `font-color`, `border-color` (цвет),
/// `flex-grow`/`flex-shrink` или сокращение `flex` (`1`, `1 0`),
/// `position` (`flow`/`absolute`), `overflow`/`overflow-x`/`overflow-y`
/// (`visible`/`hidden`/`auto`/`scroll`), `white-space` (`normal`/`nowrap`),
/// `visible` (видимость).
pub(super) fn style_token(
    generator: &Generator,
    element: &Element,
    default_font: Option<f32>,
    visible: TokenStream,
) -> syn::Result<TokenStream> {
    let width = size_attr(
        generator,
        element,
        "width",
        quote!(::light_fw::vdom::Size::Auto),
    )?;
    let height = size_attr(
        generator,
        element,
        "height",
        quote!(::light_fw::vdom::Size::Auto),
    )?;
    let min_width = optional_size_attr(generator, element, "min-width")?;
    let min_height = optional_size_attr(generator, element, "min-height")?;
    let max_width = optional_size_attr(generator, element, "max-width")?;
    let max_height = optional_size_attr(generator, element, "max-height")?;
    let flex_grow = flex_attr(generator, element, "flex-grow", 0)?;
    let flex_shrink = flex_attr(generator, element, "flex-shrink", 1)?;
    let position = position_attr(generator, element)?;
    let (overflow_x, overflow_y) = overflow_attr(generator, element)?;
    let white_space = white_space_attr(generator, element)?;
    let align_x = align_attr(
        generator,
        element,
        "position-x",
        "left",
        "right",
        quote!(::light_fw::vdom::parse_align_x),
        quote!(::light_fw::vdom::Align::Start),
    )?;
    let align_y = align_attr(
        generator,
        element,
        "position-y",
        "top",
        "bottom",
        quote!(::light_fw::vdom::parse_align_y),
        quote!(::light_fw::vdom::Align::Start),
    )?;
    let offset_x = offset_attr(generator, element, "position-offset-x")?;
    let offset_y = offset_attr(generator, element, "position-offset-y")?;
    let justify_content = align_attr(
        generator,
        element,
        "justify-content",
        "start",
        "end",
        quote!(::light_fw::vdom::parse_justify),
        quote!(::light_fw::vdom::Align::Start),
    )?;
    let align_items = align_attr(
        generator,
        element,
        "align-items",
        "start",
        "end",
        quote!(::light_fw::vdom::parse_align_items),
        quote!(::light_fw::vdom::Align::Start),
    )?;
    let padding = edges_attr(generator, element, "padding")?;
    let margin = edges_attr(generator, element, "margin")?;
    let border_size = px_attr(generator, element, "border-size")?.unwrap_or_else(|| quote!(0.0));
    let font_size = match px_attr(generator, element, "font-size")? {
        Some(value) => quote!(::std::option::Option::Some(#value)),
        None => match default_font {
            Some(default) => quote!(::std::option::Option::Some(#default)),
            None => quote!(::std::option::Option::None),
        },
    };
    let required = required_attr(generator, element)?;
    let required_indicator = required_indicator_attr(generator, element)?;
    let background = color_attr(generator, element, "background")?;
    let font_color = color_attr(generator, element, "font-color")?;
    let border_color = color_attr(generator, element, "border-color")?;
    let z = generator.z_token(element)?;
    Ok(quote!(::light_fw::vdom::Style {
        width: #width,
        height: #height,
        min_width: #min_width,
        min_height: #min_height,
        max_width: #max_width,
        max_height: #max_height,
        flex_grow: #flex_grow,
        flex_shrink: #flex_shrink,
        position: #position,
        overflow_x: #overflow_x,
        overflow_y: #overflow_y,
        white_space: #white_space,
        align_x: #align_x,
        align_y: #align_y,
        offset_x: #offset_x,
        offset_y: #offset_y,
        justify_content: #justify_content,
        align_items: #align_items,
        padding: #padding,
        margin: #margin,
        background: #background,
        font_color: #font_color,
        font_size: #font_size,
        border_color: #border_color,
        border_size: #border_size,
        visible: #visible,
        required: #required,
        required_indicator: #required_indicator,
        z: #z,
        ..::core::default::Default::default()
    }))
}

/// Разбирает `flex-grow`/`flex-shrink`, поддерживая сокращение `flex`
/// (`1`, `1 0`, `1 0 0`). В сокращении grow — первый компонент, shrink —
/// второй (по умолчанию `1`, как в CSS).
pub(super) fn flex_attr(
    generator: &Generator,
    element: &Element,
    name: &str,
    shorthand_part: usize,
) -> syn::Result<TokenStream> {
    if let Some(value) = dynamic_value(generator, element, name)? {
        return Ok(quote!(::light_fw::vdom::parse_px(&#value)));
    }
    if let Some(value) = attr_value(&element.attrs, name) {
        let number = parse_flex_number(element, name, value)?;
        return Ok(quote!(#number));
    }
    let Some(shorthand) = attr_value(&element.attrs, "flex") else {
        return Ok(if shorthand_part == 0 {
            quote!(0.0)
        } else {
            quote!(1.0)
        });
    };
    let value = shorthand
        .split_whitespace()
        .nth(shorthand_part)
        .unwrap_or("1");
    let number = parse_flex_number(element, "flex", value)?;
    Ok(quote!(#number))
}

/// Число для `flex-*`: без единиц, отрицательные значения не допускаются.
pub(super) fn parse_flex_number(element: &Element, name: &str, value: &str) -> syn::Result<f32> {
    let parsed = value.trim().parse::<f32>().map_err(|_| {
        syn::Error::new(
            Span::call_site(),
            format!(
                "invalid {name}=\"{value}\" on <{}>: expected a non-negative number",
                element.name
            ),
        )
    })?;
    if !parsed.is_finite() || parsed < 0.0 {
        return Err(syn::Error::new(
            Span::call_site(),
            format!(
                "invalid {name}=\"{value}\" on <{}>: expected a non-negative number",
                element.name
            ),
        ));
    }
    Ok(parsed)
}

/// Необязательный размер (`min-width`/`max-width` и т. п.) как `Option<Size>`.
pub(super) fn optional_size_token(element: &Element, name: &str) -> syn::Result<TokenStream> {
    Ok(match parse_size_attr(element, name)? {
        Some(token) => quote!(::std::option::Option::Some(#token)),
        None => quote!(::std::option::Option::None),
    })
}

/// Разбирает атрибут выравнивания (`position-x`/`position-y`).
pub(super) fn align_token(
    element: &Element,
    name: &str,
    start: &str,
    end: &str,
    default: TokenStream,
) -> syn::Result<TokenStream> {
    let Some(value) = attr_value(&element.attrs, name) else {
        return Ok(default);
    };
    let token = match value.trim() {
        value if value == start => quote!(::light_fw::vdom::Align::Start),
        "center" => quote!(::light_fw::vdom::Align::Center),
        value if value == end => quote!(::light_fw::vdom::Align::End),
        other => {
            return Err(syn::Error::new(
                Span::call_site(),
                format!(
                    "invalid {name}=\"{other}\" on <{}>: expected \"{start}\", \"center\" or \"{end}\"",
                    element.name
                ),
            ));
        }
    };
    Ok(token)
}

/// Разбирает атрибут смещения (`position-offset-x`/`position-offset-y`).
///
/// Допускает пиксели (`-8`, `12px`) и проценты (`10%`, в том числе со знаком).
pub(super) fn offset_token(element: &Element, name: &str) -> syn::Result<TokenStream> {
    let Some(value) = attr_value(&element.attrs, name) else {
        return Ok(quote!(::light_fw::vdom::Offset::Px(0.0)));
    };
    let value = value.trim();
    let invalid = || {
        syn::Error::new(
            Span::call_site(),
            format!(
                "invalid {name}=\"{value}\" on <{}>: expected pixels (-8, 12px) or percent (10%)",
                element.name
            ),
        )
    };
    let token = if let Some(percent) = value.strip_suffix('%') {
        let number: f32 = percent.trim().parse().map_err(|_| invalid())?;
        quote!(::light_fw::vdom::Offset::Percent(#number))
    } else {
        let number = value.strip_suffix("px").unwrap_or(value).trim();
        let number: f32 = number.parse().map_err(|_| invalid())?;
        quote!(::light_fw::vdom::Offset::Px(#number))
    };
    Ok(token)
}

/// Значение динамической привязки `:name="путь"` как `String`.
///
/// Возвращает `None`, если динамическая привязка не задана. Статический атрибут
/// `name` при этом игнорируется — динамическое значение имеет приоритет.
pub(super) fn dynamic_value(
    generator: &Generator,
    element: &Element,
    name: &str,
) -> syn::Result<Option<TokenStream>> {
    let Some(path) = attr_value(&element.attrs, &format!(":{name}")) else {
        return Ok(None);
    };
    let expr = generator.path_expr(path)?;
    Ok(Some(quote!(
        ::std::string::ToString::to_string(&(#expr))
    )))
}

/// Атрибут размера (`width`/`height`): динамический `:name="путь"` или
/// статический `name` с значением по умолчанию.
pub(super) fn size_attr(
    generator: &Generator,
    element: &Element,
    name: &str,
    default: TokenStream,
) -> syn::Result<TokenStream> {
    if let Some(value) = dynamic_value(generator, element, name)? {
        return Ok(quote!(::light_fw::vdom::parse_size_or_auto(&#value)));
    }
    Ok(parse_size_attr(element, name)?.unwrap_or(default))
}

/// Необязательный размер (`min-width`/`max-width` и т. п.) как
/// `Option<::light_fw::vdom::Size>` с учётом динамической привязки.
pub(super) fn optional_size_attr(
    generator: &Generator,
    element: &Element,
    name: &str,
) -> syn::Result<TokenStream> {
    if let Some(value) = dynamic_value(generator, element, name)? {
        return Ok(quote!(::std::option::Option::Some(
            ::light_fw::vdom::parse_size_or_auto(&#value)
        )));
    }
    optional_size_token(element, name)
}

/// Атрибут выравнивания (`position-x`, `justify-content`, …): динамический
/// `:name="путь"` (через `parse`) или статический `name` с значениями
/// `start`/`end`.
pub(super) fn align_attr(
    generator: &Generator,
    element: &Element,
    name: &str,
    start: &str,
    end: &str,
    parse: TokenStream,
    default: TokenStream,
) -> syn::Result<TokenStream> {
    if let Some(value) = dynamic_value(generator, element, name)? {
        return Ok(quote!(#parse(&#value)));
    }
    align_token(element, name, start, end, default)
}

/// Атрибут смещения (`position-offset-x`/`position-offset-y`): динамический
/// `:name="путь"` или статический `name`.
pub(super) fn offset_attr(
    generator: &Generator,
    element: &Element,
    name: &str,
) -> syn::Result<TokenStream> {
    if let Some(value) = dynamic_value(generator, element, name)? {
        return Ok(quote!(::light_fw::vdom::parse_offset(&#value)));
    }
    offset_token(element, name)
}

/// Участие в потоке (`position`): `flow`/`static`/`relative` или `absolute`.
pub(super) fn position_attr(generator: &Generator, element: &Element) -> syn::Result<TokenStream> {
    if let Some(value) = dynamic_value(generator, element, "position")? {
        return Ok(quote!(::light_fw::vdom::parse_position(&#value)));
    }
    Ok(match attr_value(&element.attrs, "position") {
        Some(value) => quote!(::light_fw::vdom::parse_position(#value)),
        None => quote!(::light_fw::vdom::Position::Flow),
    })
}

/// Переполнение (`overflow`, `overflow-x`, `overflow-y`): `overflow` задаёт
/// значение для обеих осей, `overflow-x`/`overflow-y` переопределяют ось.
pub(super) fn overflow_attr(
    generator: &Generator,
    element: &Element,
) -> syn::Result<(TokenStream, TokenStream)> {
    let base = if let Some(value) = dynamic_value(generator, element, "overflow")? {
        quote!(::light_fw::vdom::parse_overflow(&#value))
    } else {
        match attr_value(&element.attrs, "overflow") {
            Some(value) => quote!(::light_fw::vdom::parse_overflow(#value)),
            None => quote!(::light_fw::vdom::Overflow::Visible),
        }
    };
    let axis = |name: &str| -> syn::Result<TokenStream> {
        if let Some(value) = dynamic_value(generator, element, name)? {
            return Ok(quote!(::light_fw::vdom::parse_overflow(&#value)));
        }
        Ok(match attr_value(&element.attrs, name) {
            Some(value) => quote!(::light_fw::vdom::parse_overflow(#value)),
            None => base.clone(),
        })
    };
    Ok((axis("overflow-x")?, axis("overflow-y")?))
}
