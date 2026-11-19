//! Builds method bodies for the `#[template]` macro.

use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{Ident, LitStr};

use super::tokens::{collect_interpolations, collect_v_for_items, is_primitive_serde, to_ident};
use super::types::TemplateArgs;
use super::validation::{attr_value, hex_id};
use light_fw_html::{Element, Node};

pub(super) struct BuiltMethods {
    pub(super) invoke_handler: TokenStream,
    pub(super) dispatch_vmodel: TokenStream,
    pub(super) eval_field: TokenStream,
    pub(super) validate_field: TokenStream,
    pub(super) validate_form: TokenStream,
    pub(super) modal: TokenStream,
    pub(super) windows: TokenStream,
    pub(super) id_symbol: TokenStream,
    pub(super) track_files: Vec<TokenStream>,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn build_methods(
    args: &TemplateArgs,
    _ident: &Ident,
    window: &Option<Element>,
    content: &[Node],
    field_fields: &Vec<(Ident, &syn::Type)>,
    _arms: &Vec<TokenStream>,
    click_handlers: &[(TokenStream, (TokenStream, String))],
    vmodel_bindings: &Vec<(String, String, Option<String>)>,
    field_validators: &[(String, TokenStream)],
    form_validators: &[(String, TokenStream)],
    loaded_files: &[String],
) -> BuiltMethods {
    let invoke_arms: Vec<TokenStream> = click_handlers
        .iter()
        .map(|(handler_name, (call_tokens, _))| {
            quote! { #handler_name => {
                #call_tokens;
                true
            } }
        })
        .collect();

    let invoke_handler_method = if click_handlers.is_empty() {
        quote! {}
    } else {
        quote! {
            fn invoke_handler(&self, __handler: &str, __event: ::light_fw::vdom::Event) -> bool {
                match __handler {
                    #(#invoke_arms)*
                    _ => false,
                }
            }
        }
    };

    // Runtime-writeback для v-model: obходит текущий render-дерево (возвращённый
    // из `render()`), находит узел с данным ключом, читает атрибут `data-v-model`
    // и применяет writeback. Нужен только при runtime-templates (иначе статические
    // ветки `dispatch` уже обрабатывают v-model по ключу). Без привязок — пустой.
    //
    // Алгоритм:
    // 1. `self.render()` — получаем актуальное дерево (runtime-ключи).
    // 2. `find_by_key(__key)` — находим узел по ключу (позиционному).
    // 3. Читаем `data-v-model` атрибут — имя поля.
    // 4. Match по имени поля: применяем writeback через сеттер `set_<field>`.
    //    - Checkbox: toggle (`!current`)
    //    - Radio: set value (из NodeKind::Radio.value)
    //    - Select: set value (из NodeKind::Select.value, из TextChange)
    let dispatch_vmodel_method = if vmodel_bindings.is_empty() {
        quote! {}
    } else {
        // Уникальные имена полей (dedup): одно поле может быть у нескольких
        // radio (группа) или checkbox+checkbox.
        // Уникальные имена полей (dedup): одно поле может быть у нескольких
        // radio (группа). Тип берём из первого вхождения.
        let mut seen: std::collections::HashMap<&str, (&str, &Option<String>)> =
            std::collections::HashMap::new();
        for (field, ty, radio_val) in vmodel_bindings.iter() {
            seen.entry(field.as_str())
                .or_insert((ty.as_str(), radio_val));
        }
        let mut arms = Vec::new();
        for (field, &(field_ty, _)) in &seen {
            let field_str = LitStr::new(field, Span::call_site());
            let setter = Ident::new(&format!("set_{field}"), Span::call_site());
            match field_ty {
                "bool" => arms.push(quote! {
                    #field_str => {
                        // NodeKind::Checkbox всегда существует (вариант enum не
                        // зависит от фичи); если чекбокс не построен, матч
                        // просто не сработает и мы вернём false.
                        if let ::light_fw::vdom::NodeKind::Checkbox { checked, .. } = &__node.kind {
                            self.#setter(!*checked);
                            return true;
                        }
                        false
                    }
                }),
                "string" => arms.push(quote! {
                    #field_str => {
                        // Варианты NodeKind (Radio, Select) всегда существуют;
                        // если элемент не построен, матч не сработает.
                        if let ::light_fw::vdom::NodeKind::Radio { value, .. } = &__node.kind {
                            self.#setter(value.clone());
                            return true;
                        }
                        if let ::light_fw::vdom::NodeKind::Select { value, .. } = &__node.kind {
                            if let ::light_fw::vdom::Event::TextChange(__val) = __event {
                                self.#setter(__val.clone());
                            } else {
                                self.#setter(value.clone());
                            }
                            return true;
                        }
                        false
                    }
                }),
                _ => {}
            }
        }
        quote! {
            fn dispatch_vmodel(&self, __key: &str, __event: &::light_fw::vdom::Event) -> bool {
                use ::light_fw::vdom::VNode as _;
                // Только Click (checkbox/radio) и TextChange (select/text).
                if !matches!(
                    __event,
                    ::light_fw::vdom::Event::Click | ::light_fw::vdom::Event::TextChange(_)
                ) {
                    return false;
                }
                let __tree = self.render();
                let __node = match __tree.find_by_key(__key) {
                    Some(node) => node,
                    None => return false,
                };
                let __model_name = match __node.attr("data-v-model") {
                    Some(name) => name,
                    None => return false,
                };
                match __model_name {
                    #(#arms)*
                    _ => false,
                }
            }
        }
    };

    // Runtime-оценка `{{ … }}`: ветки `eval_field` для ВСЕХ идентификаторов,
    // встречающихся в шаблоне как простые пути (без точек). ТЗ 0.14.3-fix6:
    //   * если имя — поле структуры → та же логика: примитивные сериализуются,
    //     сложные не дают ветки (runtime-рендер откатывается на пустую строку,
    //     как и раньше);
    //   * если имя НЕ поле → метод из `impl App` (`fn status(&self) -> String`).
    //     Ветка `self.<метод>().to_string()` рассчитывает на проверку
    //     компилятора: несуществующий метод не скомпилируется («no method
    //     named …»). Вычисляемые значения пересчитываются при каждом рендере —
    //     после hot-swap они уже «знают» новое значение состояния (count),
    //     поэтому runtime-рендер покажет свежий текст, а не застрянет на
    //     старом значении поля.
    let field_idents: std::collections::HashSet<&Ident> =
        field_fields.iter().map(|(name, _)| name).collect();
    // Идентификаторы из шаблона собираем до создания замыкания, чтобы их
    // HashSet не пересекался по времени жизни с заимствованием `field_fields`.
    // Берём реальный контент (`content`: дети `<window>` или верхний уровень —
    // см. выше), а не сырые `nodes`, иначе в шаблоне с `<window>` интерполяции
    // лежат в детях окна и на верхнем уровне их нет.
    let v_for_vars: std::collections::HashSet<String> = collect_v_for_items(content);
    let used_idents: Vec<String> = collect_interpolations(content)
        .into_iter()
        .filter(|raw| !raw.contains('.') && !raw.is_empty())
        .filter(|raw| !v_for_vars.contains(raw))
        .collect();
    let eval_arms: Vec<proc_macro2::TokenStream> = {
        let mut arms: Vec<proc_macro2::TokenStream> = used_idents
            .iter()
            .filter_map(|raw| {
                let ident = to_ident(raw).ok()?;
                let name_str = raw.clone();
                if !field_idents.contains(&ident) {
                    // Не поле — метод (или опечатка: компилятор укажет).
                    return Some(quote! { #name_str => Some(self.#ident().to_string()), });
                }
                // Поле: только примитивные типы (как раньше) дают ветку.
                let ty = field_fields
                    .iter()
                    .find(|(n, _)| n == &ident)
                    .map(|(_, t)| t)?;
                if !is_primitive_serde(ty) {
                    return None;
                }
                Some(quote! { #name_str => Some(self.#ident().to_string()), })
            })
            .collect::<Vec<_>>();
        // v-model поля тоже должны быть доступны через eval_field (runtime-рендер
        // читает их для checked/selected/value), даже если они не встречаются
        // в интерполяциях {{ … }}.
        let mut seen: std::collections::HashSet<&str> =
            used_idents.iter().map(|s| s.as_str()).collect();
        for (field, _, _) in vmodel_bindings {
            if seen.insert(field.as_str()) {
                let ident = syn::Ident::new(field, Span::call_site());
                arms.push(quote! { #field => Some(self.#ident().to_string()), });
            }
        }
        arms
    };
    let eval_field_method = quote! {
        fn eval_field(&self, __name: &str) -> ::std::option::Option<String> {
            match __name {
                #(#eval_arms)*
                _ => None,
            }
        }
    };

    // Пользовательские проверки полей (`:validator`) и форм (`@validate`).
    let field_validator_arms = field_validators.iter().map(|(key, call)| {
        // `:validator` всегда передаёт `__value: &str` — явный аргумент,
        // поэтому `handler_call` возвращает полновесный вызов. Хвост ветки
        // обязан вернуть `Result<(), String>` (возврат метода), поэтому
        // хвост без точки с запятой.
        let body = if call.to_string().contains('(') {
            quote! { #call }
        } else {
            quote! { #call(__value) }
        };
        quote!(#key => #body,)
    });
    let validate_field_method = quote! {
        fn validate_field(
            &self,
            __field: &str,
            __value: &str,
        ) -> ::std::result::Result<(), ::std::string::String> {
            match __field {
                #(#field_validator_arms)*
                _ => ::std::result::Result::Ok(()),
            }
        }
    };
    let form_validator_arms = form_validators.iter().map(|(key, call)| {
        // `@validate` не передаёт аргументы: если в шаблоне метод указан
        // без скобок (`@validate="collect_errors"`), `handler_call`
        // возвращает путь. Чтобы хвост ветки имел тип `Option<Vec<_>>`,
        // вызов добавляется явно при отсутствии скобок.
        let body: TokenStream = if call.to_string().contains('(') {
            quote! { #call }
        } else {
            quote! { #call() }
        };
        quote! {
            #key => ::std::option::Option::Some(match #body {
                ::std::result::Result::Ok(()) => ::std::vec::Vec::new(),
                ::std::result::Result::Err(__errors) => __errors,
            }),
        }
    });
    let validate_form_method = quote! {
        #[allow(clippy::type_complexity)]
        fn validate_form(
            &self,
            __form: &str,
        ) -> ::std::option::Option<
            ::std::vec::Vec<(::std::string::String, ::std::string::String)>,
        > {
            match __form {
                #(#form_validator_arms)*
                _ => ::std::option::Option::None,
            }
        }
    };

    // Дочерний модальный компонент задаётся аргументом `modal = Компонент`.
    let modal_method = match &args.modal {
        Some(path) => quote! {
            fn modal(&self) -> ::std::option::Option<::std::boxed::Box<dyn ::light_fw::Component>> {
                ::std::option::Option::Some(::std::boxed::Box::new(#path::new()))
            }
        },
        None => quote! {},
    };

    // Окна, открываемые по идентификатору (`windows = [A, B]`).
    let windows_method = if args.windows.is_empty() {
        quote! {}
    } else {
        let factories = args.windows.iter().map(|path| {
            quote! {
                (|| -> ::std::boxed::Box<dyn ::light_fw::Component> {
                    ::std::boxed::Box::new(#path::new())
                }) as fn() -> ::std::boxed::Box<dyn ::light_fw::Component>
            }
        });
        quote! {
            fn windows(
                &self,
            ) -> ::std::vec::Vec<fn() -> ::std::boxed::Box<dyn ::light_fw::Component>> {
                ::std::vec![#(#factories),*]
            }
        }
    };

    // Уникальность `id` окон проверяется линковщиком: одинаковые идентификаторы
    // дают два одноимённых символа, и сборка завершается ошибкой.
    let id_symbol = window
        .as_ref()
        .and_then(|element| attr_value(&element.attrs, "id"))
        .filter(|id| !id.trim().is_empty())
        .map(|id| {
            let name = Ident::new(
                &format!("__light_window_id_{}", hex_id(id)),
                Span::call_site(),
            );
            quote! {
                #[used]
                #[unsafe(no_mangle)]
                static #name: u8 = 0;
            }
        })
        .unwrap_or_default();

    // РћС‚СЃР»РµР¶РёРІР°РЅРёРµ РїРµСЂРµСЃР±РѕСЂРєРё: РєР°Р¶РґС‹Р№ СѓС‡Р°СЃС‚РІСѓСЋС‰РёР№ С„Р°Р№Р» РІРєР»СЋС‡Р°РµС‚СЃСЏ РІ Р±РёРЅР°СЂРЅРёРє.
    let track_files = loaded_files.iter().map(|file| {
        quote! { const _: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/", #file)); }
    });
    BuiltMethods {
        invoke_handler: invoke_handler_method,
        dispatch_vmodel: dispatch_vmodel_method,
        eval_field: eval_field_method,
        validate_field: validate_field_method,
        validate_form: validate_form_method,
        modal: modal_method,
        windows: windows_method,
        id_symbol,
        track_files: track_files.collect(),
    }
}
