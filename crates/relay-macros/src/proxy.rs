//! Реализация атрибута `#[proxy]`.

use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::{Fields, ItemStruct, Token};

pub(super) fn expand(attr: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    if !attr.is_empty() {
        return Err(syn::Error::new(
            Span::call_site(),
            "#[proxy] не принимает аргументов",
        ));
    }

    let input: ItemStruct = syn::parse2(item)?;
    if !input.generics.params.is_empty() || input.generics.where_clause.is_some() {
        return Err(syn::Error::new(
            Span::call_site(),
            "#[proxy] не поддерживает дженерики",
        ));
    }

    let struct_attrs = &input.attrs;
    let vis = &input.vis;
    let struct_token = input.struct_token;
    let ident = &input.ident;
    let fields = match &input.fields {
        Fields::Named(fields) => fields,
        _ => {
            return Err(syn::Error::new(
                Span::call_site(),
                "#[proxy] поддерживает только структуры с именованными полями",
            ));
        }
    };

    let derive_clone = if derives_path(struct_attrs, "Clone") {
        quote!()
    } else {
        quote!(#[derive(Clone)])
    };

    let derive_debug = if derives_path(struct_attrs, "Debug") {
        quote!()
    } else {
        quote!(#[derive(Debug)])
    };

    let derive_partial_eq = if derives_path(struct_attrs, "PartialEq") {
        quote!()
    } else {
        quote!(#[derive(PartialEq)])
    };

    let mut out_fields = Vec::new();
    let mut prop_news = Vec::new();
    let mut prop_locals = Vec::new();
    let mut field_idents = Vec::new();
    let mut accessors = Vec::new();

    for (index, field) in fields.named.iter().enumerate() {
        let field_ident = field
            .ident
            .as_ref()
            .expect("поля именованные, ident обязан быть");
        if field_ident == "__relay_proxy" {
            return Err(syn::Error::new(
                field_ident.span(),
                "имя `__relay_proxy` зарезервировано библиотекой Relay",
            ));
        }

        let mut keep_attrs = Vec::new();
        let mut default_expr = None;
        for attr in &field.attrs {
            if attr.path().is_ident("default") {
                if default_expr.is_some() {
                    return Err(syn::Error::new(
                        attr.span(),
                        "атрибут #[default] указан повторно",
                    ));
                }
                let expr: syn::Expr = attr.parse_args().map_err(|_| {
                    syn::Error::new(attr.span(), "ожидается выражение: #[default(значение)]")
                })?;
                default_expr = Some(quote!(#expr));
            } else {
                keep_attrs.push(attr);
            }
        }
        let default_expr =
            default_expr.unwrap_or_else(|| quote!(::core::default::Default::default()));

        let field_vis = &field.vis;
        let field_ty = &field.ty;
        out_fields.push(quote! {
            #(#keep_attrs)*
            #field_vis #field_ident: ::relay::reactive::Property<#field_ty>
        });

        let local = syn::Ident::new(&format!("__relay_prop_{index}"), Span::call_site());
        prop_news.push(quote!(let #local = ::relay::reactive::Property::new(#default_expr);));
        prop_locals.push(local);
        field_idents.push(field_ident.clone());

        let setter_ident = syn::Ident::new(&format!("set_{field_ident}"), field_ident.span());
        let property_ident =
            syn::Ident::new(&format!("{field_ident}_property"), field_ident.span());
        accessors.push(quote! {
            /// Чтение реактивного свойства с отслеживанием зависимостей.
            pub fn #field_ident(&self) -> #field_ty {
                self.#field_ident.get()
            }

            /// Запись реактивного свойства с уведомлением обозревателей.
            pub fn #setter_ident(&self, value: #field_ty) {
                self.#field_ident.set(value)
            }

            /// Доступ к ячейке свойства для программных привязок.
            pub fn #property_ident(&self) -> &::relay::reactive::Property<#field_ty> {
                &self.#field_ident
            }
        });
    }

    Ok(quote! {
        #(#struct_attrs)*
        #derive_clone
        #derive_debug
        #derive_partial_eq
        #vis #struct_token #ident {
            #(#out_fields,)*
            __relay_proxy: ::relay::reactive::ProxyState,
        }

        impl #ident {
            /// Создаёт объект со значениями свойств по умолчанию.
            pub fn new() -> Self {
                #(#prop_news)*
                let __relay_ids: &[::relay::reactive::PropertyId] = &[#(#prop_locals.id(),)*];
                Self {
                    #(#field_idents: #prop_locals,)*
                    __relay_proxy: ::relay::reactive::ProxyState::new::<Self>(__relay_ids),
                }
            }

            #(#accessors)*
        }

        impl ::core::default::Default for #ident {
            fn default() -> Self {
                Self::new()
            }
        }

        impl ::relay::reactive::Proxy for #ident {
            fn proxy_state(&self) -> &::relay::reactive::ProxyState {
                &self.__relay_proxy
            }
        }
    })
}

fn derives_path(attrs: &[syn::Attribute], target: &str) -> bool {
    attrs
        .iter()
        .filter(|attr| attr.path().is_ident("derive"))
        .any(|attr| {
            attr.parse_args_with(Punctuated::<syn::Path, Token![,]>::parse_terminated)
                .map(|paths| {
                    paths
                        .iter()
                        .any(|path| path.segments.last().is_some_and(|seg| seg.ident == target))
                })
                .unwrap_or(false)
        })
}
