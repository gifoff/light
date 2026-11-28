//! Builds method bodies for the `#[template]` macro.

use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{Ident, LitStr};

use super::tokens::{
    collect_interpolations, collect_v_for_items, collect_v_if_names, is_primitive_serde, to_ident,
};
use super::types::TemplateArgs;
use super::validation::{attr_value, hex_id};
use light_fw_html::{Element, Node};

pub(super) struct BuiltMethods {
    pub(super) invoke_handler: TokenStream,
    pub(super) dispatch_vmodel: TokenStream,
    pub(super) eval_field: TokenStream,
    /// `eval_bool_field` для runtime `v-if="method()"`: сгенерировано только
    /// если в шаблоне есть `v-if`/`v-else-if` на имена, которые **не** поля
    /// (то есть методы-геттеры, возвращающие `bool`).
    pub(super) eval_bool_field: TokenStream,
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
    mouse_handlers: &[(String, TokenStream)],
    vmodel_bindings: &Vec<(String, String, Option<String>)>,
    field_validators: &[(String, TokenStream)],
    form_validators: &[(String, TokenStream)],
    loaded_files: &[String],
) -> BuiltMethods {
    // Две ветки на каждый `@click` (см. пояснение в pipeline.rs:185-208): по
    // ИМЕНИ обработчика и по КЛЮЧУ узла. Runtime-дерево без явного `key=`
    // ключирует кнопку по имени обработчика — до 0.20.1 генерировалась только
    // ветка по позиционному статическому ключу, поэтому `click --key increment`
    // проваливался в `_ => false`. Теперь имя сопоставляется как точный литерал,
    // а явный `key="inc_btn"` покрывает ветка по ключу узла.
    //
    // Дублировать ветки не страшно: позицияльный статический ключ уникален на
    // узел (не совпадёт ни с именем, ни с чужим ключом), а имена дедуплицируются
    // BTreeSet'ом (два узла с одним именем дают одну рабочую ветку).
    let mut seen_names: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    let mut invoke_arms: Vec<TokenStream> = Vec::new();
    for (key_token, (call_tokens, name)) in click_handlers {
        // 1. По имени обработчика — точный литерал (основной путь runtime).
        if seen_names.insert(name.as_str()) {
            let name_lit = LitStr::new(name, Span::call_site());
            let call = call_tokens.clone();
            invoke_arms.push(quote! { #name_lit => {
                #call();
                true
            } });
        }
        // 2. По ключу узла (явный `key="inc_btn"` / позиционный `"0.2"`) —
        // для случая, когда DevTools шлёт свой ключ, отличный от имени.
        invoke_arms.push(quote! { #key_token => {
            #call_tokens();
            true
        } });
    }

    // Runtime-доставка мышиных событий (`@mouseenter`/`@mouseleave`, 0.21.0):
    // runtime-парсер хранит имена ВСЕХ обработчиков в ОТДЕЛЬНЫХ атрибутах узла
    // `data-event-enter="NAME"` / `data-event-leave="NAME"` (один общий
    // `data-event` не работает — `VElement::attr` читает только первую пару,
    // см. баг 4-го раунда E2E). Каждая ветка: проверяем соответствие варианта
    // события атрибуту и вызываем метод; если ветка не совпала (другой вариант
    // или узла нет) — продолжаем обход.
    //
    // Источник — отдельный список `mouse_handlers` (codegen.rs регистрирует
    // `@mouseenter`/`@mouseleave` независимо от `@click`): чисто-мышиный
    // шаблон (tooltip при onhover, без единого клика) тоже получает ветку,
    // иначе `light devtools move/move-leave` молча игнорируется. Два разных
    // обработчика на одном элементе больше не конфликтуют: каждый вариант
    // идёт в СВОЙ атрибут и вызывает СВОЙ метод.
    let mut mouse_arms: Vec<TokenStream> = Vec::new();
    for (name, call_tokens) in mouse_handlers.iter() {
        let call = call_tokens.clone();
        for (variant, attr) in [
            ("Mouseenter", "data-event-enter"),
            ("Mouseleave", "data-event-leave"),
        ] {
            let variant_ident = syn::Ident::new(variant, Span::call_site());
            mouse_arms.push(quote! {
                if let ::light_fw::vdom::Event::#variant_ident = __event_ref
                    && let Some(node) = __root.get(__handler)
                    && node.attr(#attr).is_some_and(|n| n == #name)
                {
                    #call();
                    return true;
                }
            });
        }
    }
    // Генерация метода: нужны ЛИБО кликовые (имена/ключи), ЛИБО мышиные
    // обработчики — чисто-мышиные шаблоны тоже должны дотягиваться до
    // обработчиков через invoke_handler.
    let invoke_handler_method = if click_handlers.is_empty() && mouse_handlers.is_empty() {
        quote! {}
    } else {
        quote! {
            fn invoke_handler(&self, __handler: &str, __event: ::light_fw::vdom::Event) -> bool {
                // Runtime-доставка мыши (`@mouseenter`/`@mouseleave` на
                // контейнерах, 0.21.0): активна только при включённой
                // runtime-шаблонизации (флаг — из light-fw, без cfg-фичи
                // макросного крейта). Без data-атрибута обход инертен.
                if ::light_fw::FEATURE_RUNTIME_TEMPLATES {
                    let __root = self.render();
                    let __event_ref = &__event;
                    #(#mouse_arms)*
                }
                match __handler {
                    #(#invoke_arms)*
                    _ => false,
                }
            }
        }
    };

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
    // Имена из `v-if`/`v-else-if` тоже попадают в `eval_field`: поле, используемое
    // только в `v-if="flag"` (без `{{ flag }}`), иначе не получит ветку и
    // runtime-рендер дефолтит его к «показать» (eval_v_if → true), что держит
    // узел видимым. Добавляем их к интерполяциям (дедупликация через HashSet).
    let mut used_set: std::collections::HashSet<String> = collect_interpolations(content)
        .into_iter()
        .filter(|raw| !raw.contains('.') && !raw.is_empty())
        .filter(|raw| !v_for_vars.contains(raw))
        .collect();
    used_set.extend(collect_v_if_names(content));
    let used_idents: Vec<String> = used_set.into_iter().collect();
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

    // `eval_bool_field` для runtime `v-if="is_home"` / `v-else-if="!is_home"`:
    // имена из `v-if`/`v-else-if`, которые **не** поля структуры, считаются
    // методами-геттерами. Если пользователь назовёт несуществующее имя —
    // компилятор укажет на опечатку (метод не найден). Возврат `bool`
    // проверяется компилятором: если метод возвращает не `bool`, ошибка
    // типизации в сгенерированном коде.
    let v_if_names: std::collections::HashSet<String> = collect_v_if_names(content);
    let mut bool_arms: Vec<TokenStream> = Vec::new();
    for name in &v_if_names {
        // `to_ident` может вернуть Err для невалидных имён — в этом случае
        // пропускаем: компилятор укажет на опечатку в шаблоне.
        let ident = match to_ident(name) {
            Ok(id) => id,
            Err(_) => continue,
        };
        if field_idents.contains(&ident) {
            // Поле уже обработано в `eval_field`; дублировать не нужно.
            continue;
        }
        // Не поле → метод. Генерируем вызов `self.name()` и приводим к `bool`.
        // Если метод принимает аргументы или не возвращает `bool` — ошибка
        // типизации компилятора (это правильно: v-if требует zero-arg bool).
        let call = quote! { self.#ident() };
        bool_arms.push(quote! { #name => ::std::option::Option::Some(#call), });
    }
    let eval_bool_field_method = if bool_arms.is_empty() {
        TokenStream::new()
    } else {
        quote! {
            fn eval_bool_field(&self, __name: &str) -> ::std::option::Option<bool> {
                match __name {
                    #(#bool_arms)*
                    _ => ::std::option::Option::None,
                }
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
        eval_bool_field: eval_bool_field_method,
        validate_field: validate_field_method,
        validate_form: validate_form_method,
        modal: modal_method,
        windows: windows_method,
        id_symbol,
        track_files: track_files.collect(),
    }
}
