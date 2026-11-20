//! Р РµР°Р»РёР·Р°С†РёСЏ Р°С‚СЂРёР±СѓС‚Р° `#[template]`: С‡С‚РµРЅРёРµ HTML-С„Р°Р№Р»Р° Рё СЂР°Р·СЂРµС€РµРЅРёРµ
//! `<include>` РЅР° СЌС‚Р°РїРµ Р С”Р С•Р СР С—Р С‘Р В»РЎРЏРЎвЂ Р С‘Р С‘, РіРµРЅРµСЂР°С†РёСЏ СЂРµР°Р»РёР·Р°С†РёРё `Component`.

use proc_macro2::{Span, TokenStream};
use quote::quote;

use super::parser::Generator;
use super::tokens::is_plain_ident;
use super::validation::attr_value;
use light_fw_html::{Element, Node};

#[derive(Clone)]
pub(super) enum KeyExpr {
    /// Ключ, целиком известный на этапе компиляции.
    Lit(String),
    /// Ключ, вычисляемый во время выполнения (выражение типа `&'static str`).
    Dyn(TokenStream),
}

impl KeyExpr {
    /// Ключ дочернего узла с индексом `index`.
    pub(super) fn child(&self, index: usize) -> Self {
        match self {
            Self::Lit(base) => Self::Lit(format!("{base}.{index}")),
            Self::Dyn(expr) => {
                let suffix = format!(".{index}");
                Self::Dyn(quote!(::light_fw::vdom::intern_key(
                    &::std::format!("{}{}", #expr, #suffix)
                )))
            }
        }
    }

    /// Ключ элемента в цикле `v-for` (суффикс — индекс итерации).
    pub(super) fn iteration(&self) -> Self {
        match self {
            Self::Lit(base) => Self::Dyn(quote!(::light_fw::vdom::intern_key(
                &::std::format!("{}/{}", #base, __for_index)
            ))),
            Self::Dyn(expr) => Self::Dyn(quote!(::light_fw::vdom::intern_key(
                &::std::format!("{}/{}", #expr, __for_index)
            ))),
        }
    }

    /// Токены-выражение ключа.
    pub(super) fn token(&self) -> TokenStream {
        match self {
            Self::Lit(key) => quote!(#key),
            Self::Dyn(expr) => expr.clone(),
        }
    }
}

/// Ключ ребёнка: на верхнем уровне — просто индекс, иначе `родитель.индекс`.
pub(super) fn child_key(base: Option<&KeyExpr>, index: usize) -> KeyExpr {
    match base {
        Some(base) => base.child(index),
        None => KeyExpr::Lit(index.to_string()),
    }
}

/// Статический явный ключ элемента из атрибута `key="..."`.
///
/// Если атрибут задан — узел (и всё его поддерево) получает этот ключ вместо
/// позиционного: по нему адресуются события (в том числе команды DevTools
/// `CLICK <key>`). Значение обязано быть литералом (без `:`/интерполяции),
/// иначе макрос не может сформировать литералную ветку в `dispatch`.
pub(super) fn static_key(element: &Element, fallback: &KeyExpr) -> KeyExpr {
    match attr_value(&element.attrs, "key") {
        Some(value) if !value.trim_start().starts_with(':') => {
            KeyExpr::Lit(value.trim().to_owned())
        }
        _ => fallback.clone(),
    }
}

/// Возвращает значение директивы `name` у элемента (с обрезкой пробелов).
pub(super) fn element_directive<'a>(node: &'a Node, name: &str) -> Option<&'a str> {
    match node {
        Node::Element(element) => attr_value(&element.attrs, name),
        Node::Text(_) => None,
    }
}

/// Есть ли у элемента атрибут-флаг `name`.
pub(super) fn has_attr(node: &Node, name: &str) -> bool {
    matches!(node, Node::Element(element) if attr_value(&element.attrs, name).is_some())
}

/// Разбирает условие `v-if`/`v-else-if` (с поддержкой ведущего `!`).
pub(super) fn condition_token(generator: &Generator, condition: &str) -> syn::Result<TokenStream> {
    let condition = condition.trim();
    let (negate, path) = match condition.strip_prefix('!') {
        Some(rest) => (true, rest.trim()),
        None => (false, condition),
    };
    let expr = generator.path_expr(path)?;
    Ok(if negate { quote!(!#expr) } else { expr })
}

/// Разбирает `v-for="item in items"`.
pub(super) fn parse_for(spec: &str) -> syn::Result<(String, String)> {
    let spec = spec.trim();
    let Some((item, collection)) = spec.split_once(" in ") else {
        return Err(syn::Error::new(
            Span::call_site(),
            format!("invalid v-for=\"{spec}\" on template: expected \"item in items\""),
        ));
    };
    let item = item.trim();
    let collection = collection.trim();
    if !is_plain_ident(item) {
        return Err(syn::Error::new(
            Span::call_site(),
            format!("invalid v-for item `{item}`: expected a plain identifier"),
        ));
    }
    if collection.is_empty() {
        return Err(syn::Error::new(
            Span::call_site(),
            format!("invalid v-for=\"{spec}\": missing collection"),
        ));
    }
    Ok((item.to_string(), collection.to_string()))
}
