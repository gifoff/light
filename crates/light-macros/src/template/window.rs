//! Window-related helpers: `<window>` tag parsing, cursor values, color parsing.

use std::path::{Path, PathBuf};

use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::LitStr;

use super::parser::normalize_src;
use super::validation::attr_value;
use light_fw_html::{Element, Node};

/// Допустимые статические значения атрибута `cursor` (совпадают с
/// [`light_fw::vdom::Cursor::from_name`]).
pub(super) const CURSOR_VALUES: &[&str] = &[
    "default",
    "pointer",
    "text",
    "not-allowed",
    "wait",
    "progress",
    "help",
    "crosshair",
    "move",
    "grab",
    "grabbing",
    "ew-resize",
    "ns-resize",
    "nesw-resize",
    "nwse-resize",
];

/// Р Р°Р·РґРµР»СЏРµС‚ РєРѕСЂРЅРµРІРѕР№ <window> Рё РЎРѓР С•Р Т‘Р ВµРЎР‚Р В¶Р С‘Р СР С•Р Вµ.
pub(super) fn separate_window(nodes: Vec<Node>) -> syn::Result<(Option<Element>, Vec<Node>)> {
    if nodes.len() == 1
        && let Some(Node::Element(element)) = nodes.first()
        && element.name == "window"
    {
        return Ok((Some(element.clone()), Vec::new()));
    }
    // Р•СЃР»Рё РїРµСЂРІС‹Р№ РЎРЊР В»Р ВµР СР ВµР Р…РЎвЂљ вЂ” window, РЅРѕ РёС… РЅРµСЃРєРѕР»СЊРєРѕ вЂ” РЅРµРєРѕСЂСЂРµРєС‚РЅРѕ.
    if let Some(Node::Element(element)) = nodes.first()
        && element.name == "window"
    {
        return Err(syn::Error::new(
            Span::call_site(),
            "РЎРЊР В»Р ВµР СР ВµР Р…РЎвЂљ <window> РґРѕР»Р¶РµРЅ Р±С‹С‚СЊ Р ВµР Т‘Р С‘Р Р…РЎРѓРЎвЂљР Р†Р ВµР Р…Р Р…РЎвЂ№Р С Р С”Р С•РЎР‚Р Р…Р ВµР Р†РЎвЂ№Р С РЎС“Р В·Р В»Р С•Р С, РµСЃР»Рё РѕРЅ РїСЂРёСЃСѓС‚СЃС‚РІСѓРµС‚",
        ));
    }
    Ok((None, nodes))
}

#[derive(Clone)]
pub(super) struct WindowSpec {
    pub(super) width: Option<u32>,
    pub(super) height: Option<u32>,
    pub(super) background: Option<String>,
    pub(super) transparent: Option<String>,
    pub(super) resizable: Option<String>,
    pub(super) min_width: Option<u32>,
    pub(super) min_height: Option<u32>,
    pub(super) max_width: Option<u32>,
    pub(super) max_height: Option<u32>,
    pub(super) modal: Option<String>,
    pub(super) visible: Option<String>,
    pub(super) id: Option<String>,
    pub(super) parent: Option<String>,
    pub(super) icon: Option<String>,
}

pub(super) fn parse_window(element: &Element) -> syn::Result<WindowSpec> {
    // Атрибуты-события (`@click`, `@keydown`, …) и v-if допустимы, но не
    // влияют на параметры окна.
    Ok(WindowSpec {
        width: attr_value(&element.attrs, "width").and_then(|s| s.parse().ok()),
        height: attr_value(&element.attrs, "height").and_then(|s| s.parse().ok()),
        background: attr_value(&element.attrs, "background").map(|s| s.to_owned()),
        transparent: attr_value(&element.attrs, "transparent").map(|s| s.to_owned()),
        resizable: attr_value(&element.attrs, "resizable").map(|s| s.to_owned()),
        min_width: attr_value(&element.attrs, "min-width").and_then(|s| s.parse().ok()),
        min_height: attr_value(&element.attrs, "min-height").and_then(|s| s.parse().ok()),
        max_width: attr_value(&element.attrs, "max-width").and_then(|s| s.parse().ok()),
        max_height: attr_value(&element.attrs, "max-height").and_then(|s| s.parse().ok()),
        modal: element
            .attrs
            .iter()
            .find(|attr| attr.name == "modal")
            .map(|attr| attr.value.clone()),
        visible: element
            .attrs
            .iter()
            .find(|attr| attr.name == "visible")
            .map(|attr| attr.value.clone()),
        id: attr_value(&element.attrs, "id").map(|s| s.to_owned()),
        parent: attr_value(&element.attrs, "parent").map(|s| s.to_owned()),
        icon: attr_value(&element.attrs, "icon").map(|s| s.to_owned()),
    })
}

/// Путь к встроенному ресурсу относительно `CARGO_MANIFEST_DIR`.
pub(super) fn asset_path(value: &str) -> syn::Result<PathBuf> {
    let relative =
        normalize_src(value).map_err(|message| syn::Error::new(Span::call_site(), message))?;
    let manifest = std::env::var("CARGO_MANIFEST_DIR").map_err(|_| {
        syn::Error::new(
            Span::call_site(),
            "не удалось определить CARGO_MANIFEST_DIR",
        )
    })?;
    Ok(PathBuf::from(manifest).join(relative))
}

/// Строковый литерал пути для `include_bytes!` (прямые слэши на всех ОС).
pub(super) fn path_literal(path: &Path) -> LitStr {
    LitStr::new(
        &path.to_string_lossy().replace('\\', "/"),
        Span::call_site(),
    )
}

/// Проверяет сигнатуру PNG и читает размеры из первого блока IHDR.
pub(super) fn png_dimensions(path: &Path) -> syn::Result<(u32, u32)> {
    const SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
    let bytes = std::fs::read(path).map_err(|error| {
        syn::Error::new(
            Span::call_site(),
            format!(
                "не удалось прочитать изображение `{}`: {error}",
                path.display()
            ),
        )
    })?;
    if bytes.len() < 24 || &bytes[..8] != SIGNATURE || &bytes[12..16] != b"IHDR" {
        return Err(syn::Error::new(
            Span::call_site(),
            format!(
                "файл `{}` не является PNG с корректным заголовком IHDR",
                path.display()
            ),
        ));
    }
    let mut width = [0_u8; 4];
    let mut height = [0_u8; 4];
    width.copy_from_slice(&bytes[16..20]);
    height.copy_from_slice(&bytes[20..24]);
    Ok((u32::from_be_bytes(width), u32::from_be_bytes(height)))
}

/// Разбирает цвет `#RGB`, `#RGBA`, `#RRGGBB`, `#RRGGBBAA`, `0xRRGGBB` или
/// `0xRRGGBBAA` в `0xRRGGBBAA`.
pub(super) fn parse_hex_color(value: &str) -> syn::Result<u32> {
    let raw = value.trim();
    let digits = raw
        .strip_prefix('#')
        .or_else(|| raw.strip_prefix("0x"))
        .unwrap_or(raw);
    let error = || {
        syn::Error::new(
            Span::call_site(),
            format!("invalid color {value} (#RGB, #RGBA, #RRGGBB, #RRGGBBAA)"),
        )
    };
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(error());
    }
    let byte = |text: &str| u8::from_str_radix(text, 16).map_err(|_| error());
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
        _ => return Err(error()),
    };
    Ok(((r as u32) << 24) | ((g as u32) << 16) | ((b as u32) << 8) | a as u32)
}

pub(super) fn parse_bool(value: &str) -> bool {
    matches!(value.trim(), "true" | "1" | "yes" | "on")
}

pub(super) fn window_tokens(spec: &WindowSpec) -> syn::Result<TokenStream> {
    let mut builder = quote! { let mut cfg = ::light_fw::component::WindowConfig::default(); };
    if let Some(w) = spec.width {
        builder = quote! { #builder cfg.width = #w; };
    }
    if let Some(h) = spec.height {
        builder = quote! { #builder cfg.height = #h; };
    }
    if let Some(w) = spec.min_width {
        builder = quote! { #builder cfg.min_width = ::std::option::Option::Some(#w); };
    }
    if let Some(h) = spec.min_height {
        builder = quote! { #builder cfg.min_height = ::std::option::Option::Some(#h); };
    }
    if let Some(w) = spec.max_width {
        builder = quote! { #builder cfg.max_width = ::std::option::Option::Some(#w); };
    }
    if let Some(h) = spec.max_height {
        builder = quote! { #builder cfg.max_height = ::std::option::Option::Some(#h); };
    }
    if let Some(ref value) = spec.modal {
        let val = if value.is_empty() {
            true
        } else {
            parse_bool(value)
        };
        builder = quote! { #builder cfg.modal = #val; };
    }
    if let Some(ref value) = spec.visible {
        let val = if value.is_empty() {
            true
        } else {
            parse_bool(value)
        };
        builder = quote! { #builder cfg.visible = #val; };
    }
    let background = spec
        .background
        .as_deref()
        .map(parse_hex_color)
        .transpose()?;
    if let Some(color) = background {
        builder = quote! { #builder cfg.background = #color; };
    }
    let transparent = match spec.transparent.as_deref() {
        Some(value) => parse_bool(value),
        None => background.is_some_and(|color| color & 0xFF != 0xFF),
    };
    builder = quote! { #builder cfg.transparent = #transparent; };
    if let Some(ref r) = spec.resizable {
        let val = parse_bool(r);
        builder = quote! { #builder cfg.resizable = #val; };
    }
    if let Some(ref id) = spec.id {
        let id = id.trim();
        if !id.is_empty() {
            builder = quote! {
                #builder cfg.id = ::std::option::Option::Some(::std::string::String::from(#id));
            };
        }
    }
    if let Some(ref parent) = spec.parent {
        let parent = parent.trim();
        if !parent.is_empty() {
            builder = quote! {
                #builder cfg.parent =
                    ::std::option::Option::Some(::std::string::String::from(#parent));
            };
        }
    }
    if let Some(ref icon) = spec.icon {
        if !cfg!(feature = "image-support") {
            return Err(syn::Error::new(
                Span::call_site(),
                "атрибут icon у <window> недоступен: Cargo-фича `image-support` выключена\n\
                 help: включите её в зависимости, например \
                 `light-fw = { features = [\"image-support\"] }`",
            ));
        }
        let icon = icon.trim();
        if !icon.is_empty() {
            let path = asset_path(icon)?;
            let path = path_literal(&path);
            builder = quote! {
                #builder cfg.icon = ::std::option::Option::Some(::std::include_bytes!(#path));
            };
        }
    }
    Ok(quote! {
        {
            #builder
            cfg
        }
    })
}
