//! Атрибут `#[component(tag = "...")]`: композиция `#[template]` + `#[proxy]`
//! в плагинный элемент (ТЗ 0.19.0 Этап 4, Plugin API).
//!
//! Семантика (по ТЗ):
//! * структура **обязана** иметь атрибуты `#[template("...")]` и `#[proxy]` —
//!   иначе `compile_error!`;
//! * генерируется `impl ::light_fw::plugins::CustomElement for <Тип>`:
//!   `tag()` возвращает литерал из аргумента макроса, `render` делегирует
//!   сгенерированной `Component::render` (атрибуты и дочерние узлы плагина
//!   в Этапе 4 не используются — см. ТЗ);
//! * генерируется фабрика `CustomElementFactory` через
//!   `#[linkme::distributed_slice(REGISTERED_COMPONENTS)]`: ядро находит её
//!   по тегу при runtime-рендеринге `<тег>` из HTML и строит элемент;
//! * `crate_name` берётся из `env!("CARGO_PKG_NAME")` — имя публикуемого крейта;
//! * требование к версии ядра — `"^0.18"` (см. `check_core_version`).
//!
//! Сгенерированный код ссылается на `::light_fw::…`: пользовательский крейт
//! зависит от обёртки `light-fw` (или напрямую от `light-fw-core`, который
//! тоже объявляет `extern crate self as light_fw`).
//!
//! Требование к `linkme`: макрос `#[linkme::distributed_slice]` разворачивается
//! в крейте, где применяется `#[component]`, поэтому `linkme` должен быть
//! прямой зависимостью этого крейта (обёрка не реэкспортирует его — см.
//! `docs/` по Plugin API).

use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Ident, ItemStruct, LitStr, Token};

/// Аргументы атрибута: `#[component(tag = "...")]`.
struct ComponentArgs {
    /// Тег элемента (например, `"datepicker"`).
    tag: LitStr,
}

impl Parse for ComponentArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        // Поддерживаем оба варианта: `tag = "..."` и `(tag = "...")`.
        // В реальном вызове макроса аргумент приходит без внешних скобок,
        // но в unit-тестах `to_stream("(tag = \"...\")")` создаёт скобочную группу.
        if input.peek(syn::token::Paren) {
            let content;
            syn::parenthesized!(content in input);
            let key: Ident = content.parse()?;
            if key != "tag" {
                return Err(syn::Error::new(
                    key.span(),
                    "ожидается `tag = \"имя-тега\"`, например #[component(tag = \"datepicker\")]",
                ));
            }
            content.parse::<Token![=]>()?;
            let tag: LitStr = content.parse()?;
            Ok(Self { tag })
        } else {
            let key: Ident = input.parse()?;
            if key != "tag" {
                return Err(syn::Error::new(
                    key.span(),
                    "ожидается `tag = \"имя-тега\"`, например #[component(tag = \"datepicker\")]",
                ));
            }
            input.parse::<Token![=]>()?;
            let tag: LitStr = input.parse()?;
            if input.is_empty() {
                Ok(Self { tag })
            } else {
                Err(syn::Error::new(
                    input.span(),
                    "лишние аргументы: #[component] принимает только `tag = \"...\"`",
                ))
            }
        }
    }
}

/// Расширяет `#[component(tag = "...")] struct X` в структуру +
/// `impl CustomElement` + linkme-фабрику.
pub(super) fn expand(attr: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    let args: ComponentArgs = syn::parse2(attr)?;
    let tag_literal = args.tag.value();
    // Валидация имени тега: латиница/цифры/дефис, начинается с буквы.
    // Имя идёт в `provides`, сопоставляется в runtime и попадает в ключи узлов.
    if tag_literal.is_empty()
        || !tag_literal
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
        || !tag_literal
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic())
    {
        return Err(syn::Error::new(
            Span::call_site(),
            format!(
                "недопустимое имя тега \"{tag_literal}\": \
                 ожидаются латинские буквы, цифры и дефис, первая — буква"
            ),
        ));
    }

    let struct_item: ItemStruct = syn::parse2(item).map_err(|_| {
        syn::Error::new(
            Span::call_site(),
            "#[component] применяется только к структуре",
        )
    })?;
    let ident = &struct_item.ident;

    // Проверка обязательных атрибутов: `#[template]` и `#[proxy]`.
    // `#[component]` — композиция: без `#[template]` нет `Component::render`,
    // без `#[proxy]` нет `new()` (фабрика строит экземпляр через него).
    let has_template = struct_item
        .attrs
        .iter()
        .any(|attr| attr.path().is_ident("template"));
    if !has_template {
        return Err(syn::Error::new(
            ident.span(),
            "структура с #[component] должна иметь атрибут #[template(\"путь.html\")]: \
             он генерирует Component::render, на которую делегирует CustomElement::render",
        ));
    }
    let has_proxy = struct_item
        .attrs
        .iter()
        .any(|attr| attr.path().is_ident("proxy"));
    if !has_proxy {
        return Err(syn::Error::new(
            ident.span(),
            "структура с #[component] должна иметь атрибут #[proxy]: \
             он генерирует new(), на который ссылается фабрика элемента",
        ));
    }

    // Имя статика фабрики: уникальное внутри крейта (идент структуры + суффикс).
    // Два `#[component]` с одинаковой структурой невозможны (одна структура),
    // поэтому коллизий нет. SCREAMING_SNAKE — как у обычных static, чтобы не
    // ловить `non_upper_case_globals` в consumer-крейте.
    let mut factory_ident = String::new();
    for ch in ident.to_string().chars() {
        if ch.is_ascii_lowercase() {
            factory_ident.push(ch.to_ascii_uppercase());
            factory_ident.push('_');
        } else {
            factory_ident.push(ch);
        }
    }
    while factory_ident.ends_with('_') {
        factory_ident.pop();
    }
    factory_ident.push_str("_LIGHT_COMPONENT_FACTORY");
    let factory_ident = Ident::new(&factory_ident, ident.span());
    let tag_lit = syn::LitStr::new(&tag_literal, Span::call_site());

    Ok(quote! {
        #struct_item

        impl ::light_fw::plugins::CustomElement for #ident {
            fn tag(&self) -> &'static str {
                #tag_lit
            }

            fn render(
                &self,
                _attrs: &[(&'static str, ::std::string::String)],
                _children: &[::light_fw::vdom::VElement],
            ) -> ::light_fw::vdom::VElement {
                // Этап 4: атрибуты и дочерние узлы плагина не используются
                // (см. ТЗ); рендер делегируется статическому дереву, которое
                // собрал #[template]. Ядро перекрашивает корень в Custom(tag)
                // после возврата (см. `runtime::build_custom_element`).
                <Self as ::light_fw::component::Component>::render(self)
            }
        }

        #[linkme::distributed_slice(::light_fw::plugins::REGISTERED_COMPONENTS)]
        static #factory_ident: ::light_fw::plugins::CustomElementFactory = {
            let tag: &'static str = #tag_lit;
            ::light_fw::plugins::CustomElementFactory {
                tag,
                core_version_req: "^0.20",
                crate_name: env!("CARGO_PKG_NAME"),
                provides: &[tag],
                factory: || ::std::boxed::Box::new(#ident::new()),
            }
        };
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn to_stream(input: &str) -> TokenStream {
        input.parse().unwrap()
    }

    #[test]
    fn generates_custom_element_impl_and_factory() {
        let attr = to_stream(r#"(tag = "datepicker")"#);
        let item = to_stream(
            r#"
            #[template("templates/x.html")]
            #[proxy]
            struct Datepicker {
                selected: Option<String>,
            }
            "#,
        );
        let out = expand(attr, item).expect("expand должен пройти");
        // quote! расставляет пробелы между токенами — нормализуем для проверки.
        let text: String = out
            .to_string()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        assert!(
            text.contains("impl :: light_fw :: plugins :: CustomElement for Datepicker"),
            "не хватает impl CustomElement: {text}"
        );
        assert!(
            text.contains("\"datepicker\""),
            "тег не попадал в сгенерированный код: {text}"
        );
        assert!(
            text.contains("REGISTERED_COMPONENTS"),
            "не хватает linkme-фабрики: {text}"
        );
        assert!(
            text.contains("^0.20"),
            "нет требования к версии ядра: {text}"
        );
        assert!(
            text.contains("Datepicker :: new"),
            "фабрика должна создавать экземпляр через new(): {text}"
        );
        assert!(
            text.contains("CARGO_PKG_NAME"),
            "crate_name должен брать из env!: {text}"
        );
    }

    #[test]
    fn missing_template_attribute_is_error() {
        let attr = to_stream(r#"(tag = "x")"#);
        let item = to_stream(
            r#"
            #[proxy]
            struct X { a: i64 }
            "#,
        );
        let err = expand(attr, item).expect_err("должен быть compile_error");
        assert!(
            err.to_string().contains("template"),
            "ошибка должна упоминать template: {err}"
        );
    }

    #[test]
    fn missing_proxy_attribute_is_error() {
        let attr = to_stream(r#"(tag = "x")"#);
        let item = to_stream(
            r#"
            #[template("t.html")]
            struct X { a: i64 }
            "#,
        );
        let err = expand(attr, item).expect_err("должен быть compile_error");
        assert!(err.to_string().contains("#[proxy]"), "{err}");
    }

    #[test]
    fn invalid_tag_name_is_error() {
        // Кавычки внутри строки — `tag` не может быть пустым или содержать
        // не-ascii-символы (валидация в `expand`).
        let attr = to_stream(r#"(tag = "")"#);
        let item = to_stream(
            r#"
            #[template("t.html")]
            #[proxy]
            struct X { a: i64 }
            "#,
        );
        let err = expand(attr, item).expect_err("пустой тег должен быть ошибкой");
        assert!(err.to_string().contains("недопустимое имя тега"));
    }

    #[test]
    fn non_struct_item_is_error() {
        let attr = to_stream(r#"(tag = "x")"#);
        let item = to_stream("fn foo() {}");
        let err = expand(attr, item).expect_err("макрос применяется только к struct");
        assert!(err.to_string().contains("только к структуре"), "{err}");
    }
}
