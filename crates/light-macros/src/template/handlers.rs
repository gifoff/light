//! Р РµР°Р»РёР·Р°С†РёСЏ Р°С‚СЂРёР±СѓС‚Р° `#[template]`: С‡С‚РµРЅРёРµ HTML-С„Р°Р№Р»Р° Рё СЂР°Р·СЂРµС€РµРЅРёРµ
//! `<include>` РЅР° СЌС‚Р°РїРµ Р С”Р С•Р СР С—Р С‘Р В»РЎРЏРЎвЂ Р С‘Р С‘, РіРµРЅРµСЂР°С†РёСЏ СЂРµР°Р»РёР·Р°С†РёРё `Component`.

use proc_macro2::{Span, TokenStream};
use quote::quote;

use super::parser::Generator;
use super::tokens::{parse_call_args, split_path, to_ident};

impl Generator {
    pub(super) fn handler_call(
        &self,
        handler: &str,
        args: Option<TokenStream>,
    ) -> syn::Result<TokenStream> {
        let call = handler.trim();
        if call.contains("::") {
            let expr: syn::Expr = syn::parse_str(call).map_err(|error| {
                syn::Error::new(
                    Span::call_site(),
                    format!("неверное выражение обработчика `{call}`: {error}"),
                )
            })?;
            return Ok(quote!(#expr));
        }
        let (path, literal_args) = match call.split_once('(') {
            Some((name, rest)) => {
                let inner = rest.strip_suffix(')').ok_or_else(|| {
                    syn::Error::new(
                        Span::call_site(),
                        format!("в вызове `{call}` не закрыта скобка"),
                    )
                })?;
                (name.trim(), parse_call_args(inner)?)
            }
            None => (call, Vec::new()),
        };
        // Был ли явный вызов в шаблоне: `@click="open('x')"` — да,
        // `@click="increment"` или `@click="increment()"` — нет. Для явного
        // вызова генерируется `#expr(#args)` (или `#expr` при пустых
        // скобках), для имени метода — `#expr` (метод-ссылка, вызов делает
        // runtime).
        let is_explicit_call = call.contains('(');
        let path = path.strip_prefix("this.").unwrap_or(path).trim();
        let segments = split_path(path)?;
        let last = segments.len().saturating_sub(1);
        let args = if literal_args.is_empty() {
            args
        } else {
            Some(quote!(#(#literal_args),*))
        };
        let mut expr = quote!(self);
        for (position, segment) in segments.iter().enumerate() {
            let ident = to_ident(segment)?;
            if position == last {
                expr = quote!(#expr.#ident);
            } else {
                expr = quote!(#expr.#ident());
            }
        }
        Ok(match (is_explicit_call, args) {
            (true, Some(args)) => quote!(#expr(#args)),
            (true, None) => expr,
            (false, Some(args)) => quote!(#expr(#args)),
            (false, None) => expr,
        })
    }
}
