//! Builds `inspect_state` / `inject_state` method bodies for the `#[template]` macro.

use super::tokens::is_primitive_serde;
use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::Ident;

pub(super) struct BuiltState {
    pub(super) inspect: TokenStream,
    pub(super) inject: TokenStream,
}

pub(super) fn build_state_methods(field_fields: &Vec<(Ident, &syn::Type)>) -> BuiltState {
    let inject_bodies = field_fields
        .iter()
        .filter(|(_name, ty)| is_primitive_serde(ty))
        .map(|(name, ty)| {
            let setter = Ident::new(&format!("set_{name}"), name.span());
            let name_str = name.to_string();
            quote! {
                if let ::std::option::Option::Some(__value) = state.get(#name_str) {
                    if let ::std::result::Result::Ok(__parsed) =
                        ::light_fw::serde_json::from_value::<#ty>(__value.clone())
                    {
                        self.#setter(__parsed);
                    }
                }
            }
        })
        .collect::<Vec<_>>();
    let inject_state_method = quote! {
        fn inject_state(&mut self, state: &::light_fw::serde_json::Value) {
            #(#inject_bodies)*
        }
    };

    // `inspect_state` — без cfg (0.15.1): трейт требует его в любом наборе
    // фич. Примитивные поля инлайнятся прямо через `serde::Serialize` — это
    // не требует публичной зависимости на `::light_fw::devtools::inspect_field`
    // (которая за фичей `devtools`) и сохраняет поведение с ней, но добавляет
    // только serde-serialize-impl примитивов к размеру бинаря.
    let inspect_state_method = if field_fields.is_empty() {
        quote!()
    } else {
        let map_ident = Ident::new("__light_inspect_map", Span::call_site());
        let self_token = Ident::new("self", Span::call_site());
        let inserts = field_fields.iter().map(|(name, ty)| {
            let name_str = syn::LitStr::new(&name.to_string(), name.span());
            let getter = quote!(#self_token.#name());
            // Примитивные типы всегда реализуют `Serialize`; сложные
            // (вложенные компоненты, `Vec`, кастомные newtype) — `null`.
            // Прямой вызов `serde_json::to_value` (0.15.1): без фичи
            // `devtools` модуль `::light_fw::devtools` закрыт, а код инлайнится
            // в бинарь приложения и не требует публичного реэкспорта.
            let value = if is_primitive_serde(ty) {
                quote!(match ::light_fw::serde_json::to_value(&#getter) {
                    ::std::result::Result::Ok(v) => v,
                    ::std::result::Result::Err(_) => ::light_fw::serde_json::Value::Null,
                })
            } else {
                quote!(::light_fw::serde_json::Value::Null)
            };
            quote! {
                #map_ident.insert(::std::string::String::from(#name_str), #value);
            }
        });
        quote! {
            fn inspect_state(&self) -> ::light_fw::serde_json::Value {
                let mut #map_ident: ::light_fw::serde_json::Map<
                    ::std::string::String,
                    ::light_fw::serde_json::Value,
                > = ::light_fw::serde_json::Map::new();
                #(#inserts)*
                ::light_fw::serde_json::Value::Object(#map_ident)
            }
        }
    };
    BuiltState {
        inspect: inspect_state_method,
        inject: inject_state_method,
    }
}
