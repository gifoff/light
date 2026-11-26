//! Р РµР°Р»РёР·Р°С†РёСЏ Р°С‚СЂРёР±СѓС‚Р° `#[template]`: С‡С‚РµРЅРёРµ HTML-С„Р°Р№Р»Р° Рё СЂР°Р·СЂРµС€РµРЅРёРµ
//! `<include>` РЅР° СЌС‚Р°РїРµ Р С”Р С•Р СР С—Р С‘Р В»РЎРЏРЎвЂ Р С‘Р С‘, РіРµРЅРµСЂР°С†РёСЏ СЂРµР°Р»РёР·Р°С†РёРё `Component`.

use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::Ident;

use super::attrs::dynamic_value;
use super::keys::parse_for;
use super::parser::Generator;
use super::validation::attr_value;
use super::window::parse_hex_color;
use light_fw_html::{Element, Node, Segment};

pub(super) fn white_space_attr(
    generator: &Generator,
    element: &Element,
) -> syn::Result<TokenStream> {
    if let Some(value) = dynamic_value(generator, element, "white-space")? {
        return Ok(quote!(
            ::std::option::Option::Some(::light_fw::vdom::parse_white_space(&#value))
        ));
    }
    Ok(match attr_value(&element.attrs, "white-space") {
        Some(value) => quote!(
            ::std::option::Option::Some(::light_fw::vdom::parse_white_space(#value))
        ),
        None => quote!(::std::option::Option::None),
    })
}

/// Отступы (`padding`/`margin`): динамический `:name="путь"` задаёт базовое
/// значение для всех сторон, статические `name-*` переопределяют отдельные.
pub(super) fn edges_attr(
    generator: &Generator,
    element: &Element,
    base: &str,
) -> syn::Result<TokenStream> {
    let Some(value) = dynamic_value(generator, element, base)? else {
        return edges_token(element, base);
    };
    let scalar = quote!(::light_fw::vdom::parse_px(&#value));
    let side = |suffix: &str| -> syn::Result<TokenStream> {
        Ok(match parse_px_attr(element, &format!("{base}-{suffix}"))? {
            Some(value) => quote!(#value),
            None => scalar.clone(),
        })
    };
    let top = side("top")?;
    let right = side("right")?;
    let bottom = side("bottom")?;
    let left = side("left")?;
    Ok(quote!(::light_fw::vdom::Edges {
        top: #top,
        right: #right,
        bottom: #bottom,
        left: #left,
    }))
}

/// Атрибут в пикселях (`border-size`/`font-size`): динамический `:name="путь"`
/// или статический `name`.
pub(super) fn px_attr(
    generator: &Generator,
    element: &Element,
    name: &str,
) -> syn::Result<Option<TokenStream>> {
    if let Some(value) = dynamic_value(generator, element, name)? {
        return Ok(Some(quote!(::light_fw::vdom::parse_px(&#value))));
    }
    Ok(parse_px_attr(element, name)?.map(|value| quote!(#value)))
}

/// Цветовой атрибут (`background`/`font-color`/`border-color`) как `Option<u32>`
/// с учётом динамической привязки.
pub(super) fn color_attr(
    generator: &Generator,
    element: &Element,
    name: &str,
) -> syn::Result<TokenStream> {
    if let Some(value) = dynamic_value(generator, element, name)? {
        return Ok(quote!(::light_fw::vdom::parse_color(&#value)));
    }
    Ok(color_token(parse_color_attr(element, name)?))
}

/// Выражение флага `required` с учётом динамической привязки `:required`.
pub(super) fn required_attr(generator: &Generator, element: &Element) -> syn::Result<TokenStream> {
    if let Some(value) = dynamic_value(generator, element, "required")? {
        return Ok(quote!(::light_fw::vdom::parse_bool(&#value)));
    }
    Ok(required_token(element))
}

/// Выражение индикатора обязательности с учётом динамической привязки
/// `:required-indicator`.
pub(super) fn required_indicator_attr(
    generator: &Generator,
    element: &Element,
) -> syn::Result<TokenStream> {
    if let Some(value) = dynamic_value(generator, element, "required-indicator")? {
        return Ok(quote!(::light_fw::vdom::parse_required_indicator(&#value)));
    }
    required_indicator_token(element)
}

/// Необязательный цвет как `Option<u32>`.
pub(super) fn color_token(value: Option<u32>) -> TokenStream {
    match value {
        Some(color) => quote!(::std::option::Option::Some(#color)),
        None => quote!(::std::option::Option::None),
    }
}

/// Собирает [`light_fw::vdom::Edges`] из базового атрибута (`padding`/`margin`) и
/// переопределений по сторонам (`padding-top`, `margin-left` и т. п.).
///
/// Базовый атрибут задаёт все четыре стороны (по умолчанию `0`), атрибут с
/// суффиксом переопределяет одну сторону.
pub(super) fn edges_token(element: &Element, base: &str) -> syn::Result<TokenStream> {
    let scalar = parse_px_attr(element, base)?.unwrap_or(0.0);
    let side = |suffix: &str| -> syn::Result<f32> {
        Ok(parse_px_attr(element, &format!("{base}-{suffix}"))?.unwrap_or(scalar))
    };
    let top = side("top")?;
    let right = side("right")?;
    let bottom = side("bottom")?;
    let left = side("left")?;
    Ok(quote!(::light_fw::vdom::Edges {
        top: #top,
        right: #right,
        bottom: #bottom,
        left: #left,
    }))
}

/// Разбирает атрибут в пикселях (`12`, `12px`); `None`, если атрибута нет.
pub(super) fn parse_px_attr(element: &Element, name: &str) -> syn::Result<Option<f32>> {
    let Some(value) = attr_value(&element.attrs, name) else {
        return Ok(None);
    };
    let value = value.trim();
    let number = value.strip_suffix("px").unwrap_or(value).trim();
    let parsed = number.parse::<f32>().map_err(|_| {
        syn::Error::new(
            Span::call_site(),
            format!(
                "invalid {name}=\"{value}\" on <{}>: expected pixels (12)",
                element.name
            ),
        )
    })?;
    Ok(Some(parsed))
}

/// Разбирает цветовой атрибут (`#RRGGBB`, `#RRGGBBAA`, `0x...`).
pub(super) fn parse_color_attr(element: &Element, name: &str) -> syn::Result<Option<u32>> {
    let Some(value) = attr_value(&element.attrs, name) else {
        return Ok(None);
    };
    parse_hex_color(value).map(Some).map_err(|_| {
        syn::Error::new(
            Span::call_site(),
            format!(
                "invalid {name}=\"{value}\" on <{}>: expected #RRGGBB or #RRGGBBAA",
                element.name
            ),
        )
    })
}

/// Выражение флага `required`: наличие атрибута означает обязательность.
pub(super) fn required_token(element: &Element) -> TokenStream {
    match attr_value(&element.attrs, "required").map(str::trim) {
        None | Some("false") | Some("0") | Some("no") => quote!(false),
        Some(_) => quote!(true),
    }
}

/// Выражение индикатора обязательности (`required-indicator`).
///
/// `none` — без индикатора, `*` (по умолчанию) — звёздочка, цвет
/// (`#RRGGBB`/`#RRGGBBAA`) — акцентная полоса заданного цвета.
pub(super) fn required_indicator_token(element: &Element) -> syn::Result<TokenStream> {
    let Some(value) = attr_value(&element.attrs, "required-indicator") else {
        return Ok(quote!(::light_fw::vdom::RequiredIndicator::Asterisk));
    };
    let value = value.trim();
    let token = match value {
        "" | "*" => quote!(::light_fw::vdom::RequiredIndicator::Asterisk),
        "none" => quote!(::light_fw::vdom::RequiredIndicator::None),
        other => {
            let color = parse_hex_color(other).map_err(|_| {
                syn::Error::new(
                    Span::call_site(),
                    format!(
                        "invalid required-indicator=\"{other}\" on <{}>: expected \"none\", \"*\" \
                         or a color (#RRGGBB)",
                        element.name
                    ),
                )
            })?;
            quote!(::light_fw::vdom::RequiredIndicator::Accent(#color))
        }
    };
    Ok(token)
}

/// `state.name` в†’ `self.state().set_name(__value)`
pub(super) fn setter_expr(path: &str, value: &TokenStream) -> syn::Result<TokenStream> {
    let mut segments = split_path(path)?;
    let last = segments.pop().ok_or_else(|| {
        syn::Error::new(
            Span::call_site(),
            format!("РїСѓСЃС‚РѕР№ РїСѓС‚СЊ \"{path}\""),
        )
    })?;
    let setter = Ident::new(&format!("set_{last}"), Span::call_site());
    let mut expr = quote!(self);
    for segment in segments {
        let ident = to_ident(&segment)?;
        expr = quote!(#expr.#ident());
    }
    Ok(quote!(#expr.#setter(#value)))
}

pub(super) fn split_path(path: &str) -> syn::Result<Vec<String>> {
    let segments: Vec<String> = path.split('.').map(str::to_string).collect();
    if segments.is_empty()
        || segments
            .iter()
            .any(|segment| segment.is_empty() || !is_plain_ident(segment))
    {
        return Err(syn::Error::new(
            Span::call_site(),
            format!(
                "Р Р…Р ВµР Т‘Р С•Р С—РЎС“РЎРѓРЎвЂљР С‘Р СРЎвЂ№Р в„– РїСѓС‚СЊ \"{path}\"; РѕР¶РёРґР°РµС‚СЃСЏ С†РµРїРѕС‡РєР° СЃРІРѕР№СЃС‚РІ \
                 РёР»Рё Р СР ВµРЎвЂљР С•Р Т‘Р С•Р Р† РІРёРґР° В«state.countВ»"
            ),
        ));
    }
    Ok(segments)
}

/// Разбирает аргументы вызова вида `'settings'` или `'a', 'b'` из директивы
/// обработчика. Поддерживаются только строковые литералы в кавычках.
pub(super) fn parse_call_args(inner: &str) -> syn::Result<Vec<TokenStream>> {
    let inner = inner.trim();
    if inner.is_empty() {
        return Ok(Vec::new());
    }
    let mut args = Vec::new();
    for part in inner.split(',') {
        let part = part.trim();
        let string = part
            .strip_prefix('"')
            .and_then(|rest| rest.strip_suffix('"'))
            .or_else(|| {
                part.strip_prefix('\'')
                    .and_then(|rest| rest.strip_suffix('\''))
            });
        let Some(string) = string else {
            return Err(syn::Error::new(
                Span::call_site(),
                format!("аргумент обработчика `{part}` должен быть строкой в кавычках"),
            ));
        };
        args.push(quote!(#string));
    }
    Ok(args)
}

pub(super) fn is_plain_ident(text: &str) -> bool {
    let mut chars = text.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Обработчик `@click`, подходящий для runtime-диспетчеризации: простой
/// идентификатор (без аргументов, без `::`, без пробелов), опционально с
/// префиксом `this.` и/или пустыми скобками вызова — `increment`,
/// `this.increment()` и т. п. Сложные выражения не диспетчеризуются во runtime.
pub(super) fn is_plain_click_handler(handler: &str) -> bool {
    let trimmed = handler.trim();
    let body = trimmed.strip_prefix("this.").unwrap_or(trimmed);
    let name = body.strip_suffix("()").unwrap_or(body);
    is_plain_ident(name.trim())
}

/// Имя обработчика `@click` для runtime-диспетчеризации по имени.
///
/// Убирает скобки вызова (`"increment()"` → `"increment"`), пробельные
/// символы вокруг и ведущее `this.` — в runtime-дереве это имя становится
/// ключом узла, а статический код макроса дотягивается до него через
/// [`Component::invoke_handler`](crate::component::Component).
pub(super) fn normalize_click_name(handler: &str) -> String {
    normalize_event_handler(handler)
}

/// Обработчик мышиного события, подходящий для runtime-диспетчеризации:
/// то же определение, что [`is_plain_click_handler`] (простой идентификатор
/// без аргументов/`::`/пробелов, опционально с `this.` и/или пустыми
/// скобками). Вынесено отдельно, потому что `@mouseenter`/`@mouseleave`
/// регистрируются в `mouse_handlers`, а не в `click_handlers`.
pub(super) fn is_plain_mouse_handler(handler: &str) -> bool {
    is_plain_click_handler(handler)
}

/// Имя мышиного обработчика для runtime-диспетчеризации.
///
/// Точная нормализация, как в `light-vdom::runtime`
/// (`data-event-enter="NAME"`/`data-event-leave="NAME"`):
/// `"this.show_tip()"` → `"show_tip"`. Совпадение нормализации с парсером
/// критично: ветка `invoke_handler` сопоставляет имя из атрибута узла
/// runtime-дерева с этим именем.
pub(super) fn normalize_mouse_name(handler: &str) -> String {
    normalize_event_handler(handler)
}

/// Общая нормализация имени обработчика (`@click`, `@mouseenter`, …).
fn normalize_event_handler(handler: &str) -> String {
    let trimmed = handler.trim();
    let trimmed = trimmed.strip_prefix("this.").unwrap_or(trimmed);
    trimmed
        .strip_suffix("()")
        .map(str::trim)
        .unwrap_or(trimmed)
        .to_owned()
}

/// Все идентификаторы из `{{ … }}`-интерполяций шаблона (простые пути, без
/// точек: `{{ count }}` → `count`, `{{ status }}` → `status`; сложные
/// `{{ a.b }}` и атрибутные пути сюда не попадают). ТЗ 0.14.3-fix6: по этому
/// множеству `eval_field` знает, какие методы нужно вычислять в runtime —
/// только те, что реально используются в шаблоне. Рекурсия идёт и по тексту
/// (`Node::Text`) на любом уровне, и по дочерним элементам.
pub(super) fn collect_interpolations(nodes: &[Node]) -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::new();
    for node in nodes {
        match node {
            Node::Element(element) => {
                // BUG 0e: интерполяции в атрибутах кастомных элементов
                // (`<progress value="{{val}}">`) тоже должны попадать в
                // `eval_field`, иначе runtime не сможет их разрешить.
                for attr in &element.attrs {
                    if let Some(field) = extract_attr_interpolation(&attr.value) {
                        out.insert(field);
                    }
                }
                out.extend(collect_interpolations(&element.children));
            }
            Node::Text(segments) => {
                for segment in segments {
                    if let light_fw_html::Segment::Path(path) = segment {
                        out.insert(path.clone());
                    }
                }
            }
        }
    }
    out
}

/// Извлекает простое имя поля из значения атрибута, если оно содержит ровно
/// один `{{ … }}`-блок (BUG 0e). Возвращает `None` для статических значений
/// и сложных выражений (с точками, несколькими блоками и т.п.).
fn extract_attr_interpolation(value: &str) -> Option<String> {
    let (start, end) = match (value.find("{{"), value.find("}}")) {
        (Some(s), Some(e)) if s < e && !value[e + 2..].contains("{{") => (s, e),
        _ => return None,
    };
    let field = value[start + 2..end].trim();
    if field.is_empty() || field.contains('.') {
        return None;
    }
    Some(field.to_owned())
}

/// Все имена переменных `v-for` в шаблоне. Эти идентификаторы — локальные
/// переменные цикла, а не методы/поля компонента, поэтому их нужно исключить
/// из веток `eval_field`.
pub(super) fn collect_v_for_items(nodes: &[Node]) -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::new();
    for node in nodes {
        if let Node::Element(element) = node {
            if let Some(spec) = attr_value(&element.attrs, "v-for")
                && let Ok((item, _)) = parse_for(spec)
            {
                out.insert(item);
            }
            out.extend(collect_v_for_items(&element.children));
        }
    }
    out
}

/// Все имена, используемые в атрибутах `v-if` / `v-else-if` (без `!`-префикса).
/// Эти имена могут быть полями **или** методами-геттерами (`is_home()`),
/// поэтому их нужно собирать отдельно от интерполяций `{{ … }}`, чтобы
/// макрос мог сгенерировать ветку `eval_bool_field` для методов.
pub(super) fn collect_v_if_names(nodes: &[Node]) -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::new();
    for node in nodes {
        if let Node::Element(element) = node {
            for attr in &element.attrs {
                let tag = &attr.name;
                if tag == "v-if" || tag == "v-else-if" {
                    let value = attr.value.trim().trim_start_matches('!');
                    // Берём только простые идентификаторы (без точек, скобок).
                    let name = value.trim();
                    if !name.is_empty()
                        && !name.contains('.')
                        && !name.contains('(')
                        && !name.contains(|c: char| c.is_whitespace())
                    {
                        out.insert(name.to_string());
                    }
                }
            }
            out.extend(collect_v_if_names(&element.children));
        }
    }
    out
}

/// Является ли тип поля структуры примитивным, который всегда реализует
/// `serde::Serialize` и безопасно сериализуется «через геттер» в снимке
/// состояния (ТЗ 0.13-D-fix, задача 2/3): числа (`i8…i128`, `u8…u128`,
/// `usize`, `isize`, `f32`, `f64`), `bool`, `char`, `str`, `String`,
/// `Cow`, а также их `Option`-обёртки (`Option<i64>` и т. п.).
///
/// Вложенные прокси-компоненты, `Vec` и прочие сложные типы не попадают
/// сюда: их `inspect_state` показывает как `null`, что не требует
/// `#[derive(Serialize)]` нигде и позволяет собрать `--all-features`.
pub(super) fn is_primitive_serde(ty: &syn::Type) -> bool {
    /// Базовый (не-`Option`) примитивный тип из списка выше.
    fn base(ty: &syn::Type) -> bool {
        let syn::Type::Path(path) = ty else {
            return false;
        };
        if path.qself.is_some() {
            return false;
        }
        let segs: Vec<String> = path
            .path
            .segments
            .iter()
            .map(|s| s.ident.to_string())
            .collect();
        // Точное имя типа (без путей-префиксов `std::...`).
        let last = segs.last().cloned().unwrap_or_default();
        const PRIM: &[&str] = &[
            "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128", "usize",
            "f32", "f64", "bool", "char", "str", "String", "Cow",
        ];
        PRIM.contains(&last.as_str())
    }
    // `Option<T>` допустим, если `T` примитивный: `Some(v)` сериализуется
    // стандартно, а ветвление на `None` генерировать не нужно —
    // `inspect_field(&self.field())` принимает `&Option<T>` напрямую.
    let syn::Type::Path(path) = ty else {
        return base(ty);
    };
    if path.qself.is_some() {
        return false;
    }
    let segs: Vec<&syn::PathSegment> = path.path.segments.iter().collect();
    if segs.len() != 2 || segs[0].ident != "Option" {
        return base(ty);
    }
    match &segs[1].arguments {
        syn::PathArguments::AngleBracketed(args) => {
            if let Some(syn::GenericArgument::Type(inner)) = args.args.first() {
                base(inner)
            } else {
                false
            }
        }
        _ => false,
    }
}

pub(super) fn to_ident(segment: &str) -> syn::Result<Ident> {
    syn::parse_str::<Ident>(segment).map_err(|_| {
        syn::Error::new(
            Span::call_site(),
            format!("Р Р…Р ВµР Т‘Р С•Р С—РЎС“РЎРѓРЎвЂљР С‘Р СР С•Р Вµ Р С‘Р СРЎРЏ `{segment}` РІ РїСѓС‚Рё С€Р°Р±Р»РѕРЅР°"),
        )
    })
}

/// Р вЂќР ВµРЎвЂљР ВµРЎР‚Р СР С‘Р Р…Р С‘РЎР‚РЎС“Р ВµРЎвЂљ РЎРѓР С•Р Т‘Р ВµРЎР‚Р В¶Р С‘Р СР С•Р Вµ `<text>`, `<button>`: С‚РѕР»СЊРєРѕ С‚РµРєСЃС‚ Рё РёРЅС‚РµСЂРїРѕР»СЏС†РёСЏ.
pub(super) fn text_segments(children: &[Node], element: &str) -> syn::Result<Vec<Segment>> {
    let mut segments = Vec::new();
    for child in children {
        match child {
            Node::Text(parts) => segments.extend(parts.iter().cloned()),
            Node::Element(nested) => {
                return Err(syn::Error::new(
                    Span::call_site(),
                    format!(
                        "РЎРЊР В»Р ВµР СР ВµР Р…РЎвЂљ <{element}> Р СР С•Р В¶Р ВµРЎвЂљ СЃРѕРґРµСЂР¶Р°С‚СЊ С‚РѕР»СЊРєРѕ С‚РµРєСЃС‚ Рё {{{{ вЂ¦ }}}}, \
                         Р° РЅР°Р№РґРµРЅ <{}>",
                        nested.name
                    ),
                ));
            }
        }
    }
    Ok(segments)
}
