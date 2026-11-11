//! Р РµР°Р»РёР·Р°С†РёСЏ Р°С‚СЂРёР±СѓС‚Р° `#[template]`: С‡С‚РµРЅРёРµ HTML-С„Р°Р№Р»Р° Рё СЂР°Р·СЂРµС€РµРЅРёРµ
//! `<include>` РЅР° СЌС‚Р°РїРµ Р С”Р С•Р СР С—Р С‘Р В»РЎРЏРЎвЂ Р С‘Р С‘, РіРµРЅРµСЂР°С†РёСЏ СЂРµР°Р»РёР·Р°С†РёРё `Component`.

use std::path::{Path, PathBuf};

use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Ident, ItemStruct, LitStr, Token};

use light_fw_html::{Attr, Element, Node, Segment};

/// Аргументы атрибута
/// `#[template("путь.html", modal = Компонент, window = Компонент, windows = [A, B])]`.
struct TemplateArgs {
    path: LitStr,
    /// Компонент дочернего модального окна (необязательный).
    modal: Option<syn::Path>,
    /// Компонент дополнительного главного окна (необязательный).
    window: Option<syn::Path>,
    /// Компоненты окон, открываемых по идентификатору через `open("id")`.
    windows: Vec<syn::Path>,
}

impl Parse for TemplateArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let path: LitStr = input.parse()?;
        let mut modal = None;
        let mut window = None;
        let mut windows = Vec::new();
        while input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
            if input.is_empty() {
                break;
            }
            let key: Ident = input.parse()?;
            input.parse::<Token![=]>()?;
            if key == "modal" {
                modal = Some(input.parse()?);
            } else if key == "window" {
                window = Some(input.parse()?);
            } else if key == "windows" {
                let content;
                syn::bracketed!(content in input);
                windows = syn::punctuated::Punctuated::<syn::Path, Token![,]>::parse_terminated(
                    &content,
                )?
                .into_iter()
                .collect();
            } else {
                return Err(syn::Error::new(
                    key.span(),
                    "неизвестный аргумент #[template]: ожидается `modal = Компонент`, `window = Компонент` или `windows = [A, B]`",
                ));
            }
        }
        Ok(Self {
            path,
            modal,
            window,
            windows,
        })
    }
}

/// Р РµР·СѓР»СЊС‚Р°С‚ Р·Р°РіСЂСѓР·РєРё С€Р°Р±Р»РѕРЅР°: СѓР·Р»С‹ СЃ РЎР‚Р В°Р В·РЎР‚Р ВµРЎв‚¬РЎвЂР Р…Р Р…РЎвЂ№Р СР С‘ include Рё СЃРїРёСЃРѕРє
/// СѓС‡Р°СЃС‚РІСѓСЋС‰РёС… С„Р°Р№Р»РѕРІ (РґР»СЏ РѕС‚СЃР»РµР¶РёРІР°РЅРёСЏ РїРµСЂРµСЃР±РѕСЂРєРё).
struct Loaded {
    nodes: Vec<Node>,
    /// РџСѓС‚Рё Рє РЎвЂћР В°Р в„–Р В»Р В°Р С РѕС‚РЅРѕСЃРёС‚РµР»СЊРЅРѕ РєРѕСЂРЅСЏ РїР°РєРµС‚Р° (РёР»Рё Р°Р±СЃРѕР»СЋС‚РЅС‹Рµ).
    files: Vec<String>,
}

pub(super) fn expand(attr: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    let args: TemplateArgs = syn::parse2(attr).map_err(|error| {
        syn::Error::new(
            error.span(),
            "ожидается путь к шаблону: #[template(\"templates/app.html\", modal = Компонент)]",
        )
    })?;
    let window_factory_method = match &args.window {
        Some(path) => quote! {
            fn window_factory(
                &self,
            ) -> ::std::option::Option<::std::boxed::Box<dyn ::light_fw::component::Component>> {
                ::std::option::Option::Some(::std::boxed::Box::new(#path::new()))
            }
        },
        None => quote!(),
    };
    let template_path = args.path;
    let relative = normalize_src(&template_path.value())
        .map_err(|message| syn::Error::new(template_path.span(), message))?;

    let struct_item: ItemStruct = syn::parse2(item).map_err(|_| {
        syn::Error::new(
            Span::call_site(),
            "#[template] Р СР С•Р В¶Р Р…Р С• Р С—РЎР‚Р С‘Р СР ВµР Р…РЎРЏРЎвЂљРЎРЉ С‚РѕР»СЊРєРѕ Рє СЃС‚СЂСѓРєС‚СѓСЂРµ",
        )
    })?;
    let ident = struct_item.ident.clone();

    // Имя и тип каждого поля структуры. Каждое поле — *состояние*: оно
    // сохраняется в `state.json` через `inspect_state` и восстанавливается из
    // него через `inject_state`. Вычисляемые значения (computed) — это методы
    // в `impl App` (`fn status(&self) -> String`): они никогда не сохраняются
    // и пересчитываются при каждом рендере; `eval_field` узнаёт о них из
    // текста шаблона (ТЗ 0.14.3-fix6).
    //
    // Тип нужен, чтобы отличить примитивы (`i64`, `bool`, `String`, …) от
    // сложных типов: примитивы сериализуются через `inspect_field`, остальные
    // — как `null` (не требует `#[derive(Serialize)]` на пользовательских
    // типах).
    let field_fields: Vec<(Ident, &syn::Type)> = match &struct_item.fields {
        syn::Fields::Named(fields) => fields
            .named
            .iter()
            .map(|field| {
                (
                    field
                        .ident
                        .clone()
                        .expect("поля именованные, ident обязан быть"),
                    &field.ty,
                )
            })
            .collect::<Vec<_>>(),
        _ => Vec::new(),
    };

    // Метод `inject_state` (фича `devtools`): восстанавливает значения полей из
    // JSON-объекта, снятого `inspect_state`, через сеттеры `set_<поле>`
    // (их генерирует `#[proxy]`). Не-примитивные поля (и `Option`) пропущены,
    // как и в `inspect_state`. Геттер здесь не читается — только сеттер,
    // поэтому дробного заёма (`&self` на геттере + `&mut self` на сеттере) нет.
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

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").map_err(|_| {
        syn::Error::new(
            Span::call_site(),
            "РЅРµ СѓРґР°Р»РѕСЃСЊ РѕРїСЂРµРґРµР»РёС‚СЊ CARGO_MANIFEST_DIR",
        )
    })?;
    let manifest_dir = PathBuf::from(manifest_dir);
    let full_path = manifest_dir.join(&relative);
    let mut stack = Vec::new();
    let loaded = load_nodes(&full_path, &mut stack)?;

    // РљРѕСЂРЅРµРІРѕР№ РЎРЊР В»Р ВµР СР ВµР Р…РЎвЂљ `<window>` РѕРїРёСЃС‹РІР°РµС‚ РЎРѓР С‘РЎРѓРЎвЂљР ВµР СР Р…Р С•Р Вµ РѕРєРЅРѕ; РµРіРѕ РґРµС‚Рё вЂ”
    // РЎРѓР С•Р Т‘Р ВµРЎР‚Р В¶Р С‘Р СР С•Р Вµ РёРЅС‚РµСЂС„РµР№СЃР°. РџСѓСЃС‚РѕР№ С€Р°Р±Р»РѕРЅ (Р±РµР· СѓР·Р»РѕРІ) Р Т‘Р В°РЎвЂРЎвЂљ `None` вЂ”
    // РѕРєРЅРѕ РЅРµ РЎРѓР С•Р В·Р Т‘Р В°РЎвЂРЎвЂљРЎРѓРЎРЏ.
    let (window, nodes) = separate_window(loaded.nodes)?;
    let window_config = match &window {
        Some(element) => {
            let spec = parse_window(element)?;
            window_tokens(&spec)?
        }
        None => quote!(::light_fw::component::WindowConfig::default()),
    };
    let render_body = if window.is_some() || !nodes.is_empty() {
        quote! {
            ::light_fw::vdom::VElement::container(
                "",
                ::light_fw::vdom::NodeKind::Window,
                __children,
            )
            .with_window(#window_config)
        }
    } else {
        quote! {
            ::light_fw::vdom::VElement::container(
                "",
                ::light_fw::vdom::NodeKind::Column,
                __children,
            )
        }
    };

    let mut generator = Generator::default();
    if let Some(element) = &window {
        generator.push_events(element, &quote!(""))?;
    }
    let content: &[Node] = match &window {
        Some(element) => &element.children,
        None => &nodes,
    };
    let pushes = generator.gen_children(content, None)?;
    let Generator {
        arms,
        field_validators,
        form_validators,
        click_handlers,
        vmodel_bindings,
        ..
    } = generator;

    // Runtime-шаблон: путь к файлу (уникальная константа для каждого компонента).
    let template_rel = &relative;
    let const_name = Ident::new(
        &format!("__light_template_path_{}", ident),
        Span::call_site(),
    );
    let self_template_path = quote! {
        const #const_name: &'static str =
            concat!(env!("CARGO_MANIFEST_DIR"), "/", #template_rel);
    };

    // Runtime-рендер: проверка в начале `render()` — если файл на диске изменился
    // и успешно разобрался, используем его вместо статического дерева.
    let runtime_render_check = quote! {
        if ::light_fw::FEATURE_RUNTIME_TEMPLATES {
            if let Some(__rt) = ::light_fw::runtime::render_dynamic(
                #const_name,
                include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/", #template_rel)),
                self,
            ) {
                return __rt;
            }
        }
    };

    // Runtime-диспетчеризация: ветви `dispatch` (по ключу узла из статического
    // дерева) и `invoke_handler`. Ключи могут быть произвольными выражениями
    // (`intern_key(format!(…))`), поэтому в `dispatch` — `if`-цепочка.
    //
    // `invoke_handler` получает две ветки на каждый `@click`: по ИМЕНИ
    // обработчика (так runtime-дерево дотянется при отсутствии явного `key`) и
    // по КЛЮЧУ узла (явный `key="inc_btn"` — runtime key, ≠ имени). Дедупликация
    // не нужна: два узла с одним и тем же именем дают одну рабочую ветку.
    // Runtime-дерево использует имя обработчика (без скобок) как ключ узла,
    // поэтому `invoke_handler` дублирует каждую ветку `@click` по имени и по
    // ключу узла. Ключи могут быть произвольными выражениями
    // (`intern_key(format!(…))`), поэтому в `dispatch` — `if`-цепочка
    // (собранная в `arms` выше, в `push_events`).
    // Ветки `invoke_handler`: две формы на каждый `@click`.
    //   1. Имя обработчика (базовый, без скобок) — точный литерал:
    //      runtime-дерево использует имя обработчика как ключ узла, если
    //      явный `key` не задан (`@click="increment"` → runtime key "increment").
    //   2. Ключ узла (token `&'static str`) — сравнение с аргументом:
    //      покрывает случай явного `key="inc_btn"`, когда статический
    //      `dispatch` уже обработал клик, а runtime-дерево всё равно хочет
    //      диспетчеризовать по своему ключу (DevTools).
    //
    // `#call` возвращается из `handler_call` в двух формах:
    //   - аргументные  → полновесный вызов  `self.open('settings')`;
    //   - безаргументные → путь              `self.increment` (вызов ниже).
    // Хвост ветки `match` обязан вернуть `bool`, поэтому для безаргументных
    // обрабатываем `{ #call() }` (явный вызов) и для аргументных просто `#call`.
    // Дедупликация по имени не нужна: два узла с одним именем дают одну ветку.
    let invoke_arms: Vec<TokenStream> = click_handlers
        .iter()
        .flat_map(|(key, (call, name))| {
            let name_lit = LitStr::new(name, Span::call_site());
            // Ветка `match` должна вернуть `bool`. Все обработчики здесь —
            // простые имена методов (проверено `is_plain_click_handler`),
            // поэтому вызываем явно и возвращаем `true`.
            let body = quote! { #call(); true };
            vec![
                quote! {
                    #name_lit => { #body }
                },
                quote! {
                    _ if #key == __handler => { #body }
                },
            ]
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
            seen.entry(field.as_str()).or_insert((ty.as_str(), radio_val));
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
        let mut seen: std::collections::HashSet<&str> = used_idents.iter().map(|s| s.as_str()).collect();
        for (field, _, _) in &vmodel_bindings {
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
    let track_files = loaded.files.iter().map(|file| {
        quote! { const _: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/", #file)); }
    });

    Ok(quote! {
        #struct_item
        #self_template_path

        #(#track_files)*
        #id_symbol

        impl ::light_fw::Component for #ident {
            fn as_any_mut(&mut self) -> &mut dyn ::std::any::Any {
                self
            }

            fn render(&self) -> ::light_fw::vdom::VElement {
                #runtime_render_check
                let mut __children = ::std::vec::Vec::new();
                #(#pushes)*
                #render_body
            }

            fn dispatch(&self, __key: &str, __event: ::light_fw::vdom::Event) -> bool {
                #[allow(unused_imports)]
                use ::light_fw::dom::Dom;
                #[allow(unused_imports)]
                use ::light_fw::vdom::{VNode as _, VWindow as _};
                let mut __handled = false;
                #(#arms)*
                if !__handled && ::light_fw::FEATURE_RUNTIME_TEMPLATES {
                    // 1. @click handlers по имени (runtime-дерево дотягивается
                    //    до обработчика по имени).
                    // 2. v-model writeback: runtime-дерево использует
                    //    позиционные ключи, отличные от статических — ищем
                    //    узел по ключу, читаем data-v-model, применяем сеттер.
                    __handled = self.dispatch_by_name(__key, __event.clone())
                        || self.dispatch_vmodel(__key, &__event);
                }
                __handled
            }

            #modal_method
            #window_factory_method
            #windows_method
            #validate_field_method
            #validate_form_method
            #inspect_state_method
            #inject_state_method
            #eval_field_method
            #invoke_handler_method
            #dispatch_vmodel_method
        }
    })
}

/// РџСЂРѕРІРµСЂСЏРµС‚ РїСѓС‚СЊ РёР· `src`/Р°С‚СЂРёР±СѓС‚Р° С€Р°Р±Р»РѕРЅР°: С‚РѕР»СЊРєРѕ РѕС‚РЅРѕСЃРёС‚РµР»СЊРЅС‹Рµ РїСѓС‚Рё.
fn normalize_src(value: &str) -> Result<String, String> {
    if value.starts_with('/') || value.contains(':') {
        return Err(format!(
            "РїСѓС‚СЊ \"{value}\" РґРѕР»Р¶РµРЅ Р±С‹С‚СЊ Р С•РЎвЂљР Р…Р С•РЎРѓР С‘РЎвЂљР ВµР В»РЎРЉР Р…РЎвЂ№Р С (РѕС‚ РєРѕСЂРЅСЏ РїР°РєРµС‚Р°)"
        ));
    }
    Ok(value.replace('\\', "/"))
}

/// Р РµРєСѓСЂСЃРёРІРЅРѕ С‡РёС‚Р°РµС‚ С€Р°Р±Р»РѕРЅ Рё СЂР°Р·СЂРµС€Р°РµС‚ РЎРЊР В»Р ВµР СР ВµР Р…РЎвЂљРЎвЂ№ `<include src="...">`.
///
/// Include СЂР°Р·СЂРµС€Р°РµС‚СЃСЏ РѕС‚РЅРѕСЃРёС‚РµР»СЊРЅРѕ С„Р°Р№Р»Р°, РєРѕС‚РѕСЂС‹Р№ РµРіРѕ СЃРѕРґРµСЂР¶РёС‚; РїРѕРІС‚РѕСЂРЅРѕРµ
/// РІРєР»СЋС‡РµРЅРёРµ С‚РѕРіРѕ Р¶Рµ С„Р°Р№Р»Р° РІ С†РµРїРѕС‡РєРµ СЃС‡РёС‚Р°РµС‚СЃСЏ РѕС€РёР±РєРѕР№.
fn load_nodes(path: &Path, stack: &mut Vec<PathBuf>) -> syn::Result<Loaded> {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    if let Some(position) = stack.iter().position(|entry| entry == &canonical) {
        let mut chain = String::new();
        for entry in &stack[position..] {
            chain.push_str(&entry.display().to_string());
            chain.push_str(" в†’ ");
        }
        chain.push_str(&canonical.display().to_string());
        return Err(syn::Error::new(
            Span::call_site(),
            format!("С†РёРєР» include РІ С€Р°Р±Р»РѕРЅРµ: {chain}"),
        ));
    }

    let source = std::fs::read_to_string(path).map_err(|error| {
        syn::Error::new(
            Span::call_site(),
            format!(
                "РЅРµ СѓРґР°Р»РѕСЃСЊ РїСЂРѕС‡РёС‚Р°С‚СЊ С€Р°Р±Р»РѕРЅ `{}`: {error}",
                path.display()
            ),
        )
    })?;
    let nodes = light_fw_html::parse(&source).map_err(|error| {
        syn::Error::new(
            Span::call_site(),
            format!("С€Р°Р±Р»РѕРЅ `{}`: {error}", path.display()),
        )
    })?;

    let mut loaded = Loaded {
        nodes: Vec::new(),
        files: Vec::new(),
    };
    if let Ok(manifest) = std::env::var("CARGO_MANIFEST_DIR")
        && let Ok(shown) = path.strip_prefix(&manifest)
    {
        loaded
            .files
            .push(shown.to_string_lossy().replace('\\', "/"));
    }

    stack.push(canonical);
    let base = path.parent().unwrap_or(Path::new("."));
    for node in nodes {
        let resolved = resolve_node(node, base, stack, &mut loaded.files)?;
        loaded.nodes.extend(resolved);
    }
    stack.pop();
    Ok(loaded)
}

/// Р Р°Р·СЂРµС€Р°РµС‚ РѕРґРёРЅ СѓР·РµР»: `<include>` Р В·Р В°Р СР ВµР Р…РЎРЏР ВµРЎвЂљРЎРѓРЎРЏ РЎРѓР С•Р Т‘Р ВµРЎР‚Р В¶Р С‘Р СРЎвЂ№Р С С„Р°Р№Р»Р°,
/// РґРµС‚Рё РЎРЊР В»Р ВµР СР ВµР Р…РЎвЂљР С•Р Р† РѕР±С…РѕРґСЏС‚СЃСЏ СЂРµРєСѓСЂСЃРёРІРЅРѕ.
fn resolve_node(
    node: Node,
    base: &Path,
    stack: &mut Vec<PathBuf>,
    files: &mut Vec<String>,
) -> syn::Result<Vec<Node>> {
    match node {
        Node::Text(_) => Ok(vec![node]),
        Node::Element(element) if element.name == "include" => {
            validate_include(&element.attrs)?;
            if !element.children.is_empty() {
                return Err(syn::Error::new(
                    Span::call_site(),
                    "<include> РЅРµ Р СР С•Р В¶Р ВµРЎвЂљ СЃРѕРґРµСЂР¶Р°С‚СЊ РІР»РѕР¶РµРЅРЅС‹С… РЎРЊР В»Р ВµР СР ВµР Р…РЎвЂљР С•Р Р† РёР»Рё С‚РµРєСЃС‚",
                ));
            }
            let src = element
                .attrs
                .iter()
                .find(|attr| attr.name == "src")
                .map(|attr| attr.value.clone())
                .unwrap_or_default();
            let src = normalize_src(&src)
                .map_err(|message| syn::Error::new(Span::call_site(), message))?;
            let target = base.join(&src);
            let mut included = load_nodes(&target, stack)?;
            files.append(&mut included.files);
            Ok(included.nodes)
        }
        Node::Element(mut element) => {
            let mut children = Vec::with_capacity(element.children.len());
            for child in std::mem::take(&mut element.children) {
                children.extend(resolve_node(child, base, stack, files)?);
            }
            element.children = children;
            Ok(vec![Node::Element(element)])
        }
    }
}

fn validate_include(attrs: &[Attr]) -> syn::Result<()> {
    if attrs.iter().filter(|attr| attr.name == "src").count() > 1 {
        return Err(syn::Error::new(
            Span::call_site(),
            "Р°С‚СЂРёР±СѓС‚ `src` Сѓ <include> СѓРєР°Р·Р°РЅ РїРѕРІС‚РѕСЂРЅРѕ",
        ));
    }
    if !attrs.iter().any(|attr| attr.name == "src") {
        return Err(syn::Error::new(
            Span::call_site(),
            "<include> С‚СЂРµР±СѓРµС‚ Р°С‚СЂРёР±СѓС‚ src=\"РїСѓС‚СЊ/Рє/С€Р°Р±Р»РѕРЅСѓ.html\"",
        ));
    }
    Ok(())
}

/// РЎС‚СЂРѕРёС‚ РІС‹СЂР°Р¶РµРЅРёСЏ `__children.push(вЂ¦)` РґР»СЏ СѓР·Р»РѕРІ Рё СЂРµРіРёСЃС‚СЂР°С†РёСЋ
/// РѕР±СЂР°Р±РѕС‚С‡РёРєРѕРІ СЃРѕР±С‹С‚РёР№.
#[derive(Default)]
struct Generator {
    arms: Vec<TokenStream>,
    /// РРјРµРЅР° локальных переменных `v-for`, видимых в текущей вложенности.
    scopes: Vec<String>,
    /// Пользовательские проверки полей: `(ключ узла, вызов метода)`.
    field_validators: Vec<(String, TokenStream)>,
    /// Пользовательские проверки форм: `(ключ формы, вызов метода)`.
    form_validators: Vec<(String, TokenStream)>,
    /// Обработчики `@click` для runtime-диспетчеризации: `(ключ узла как токен,
    /// вызов метода)`. Токен ключа — выражение типа `&'static str`.
    /// Ветви `@click` для runtime-диспетчеризации: (ключ узла из статического
    /// дерева, (вызов метода, имя обработчика без скобок)). Имя используется в
    /// `invoke_handler`, где runtime-дерево дотягивается до обработчика по имени.
    click_handlers: Vec<(TokenStream, (TokenStream, String))>,
    /// Привязки v-model для runtime-writeback: (имя поля, тип поля, значение radio).
    /// Checkbox → (field, "bool", None); Radio → (field, "string", Some(value));
    /// Select → (field, "string", None). Используется в `dispatch_vmodel`,
    /// который обходит runtime-дерево по ключу и записывает новое значение.
    vmodel_bindings: Vec<(String, String, Option<String>)>,
}

/// Ключ узла: известный на этапе компиляции либо вычисляемый во время работы
/// (`v-for`). Статические ключи остаются `&'static str`-литералами, динамические
/// интернируются через [`light_fw::vdom::intern_key`].
#[derive(Clone)]
enum KeyExpr {
    /// Ключ, целиком известный на этапе компиляции.
    Lit(String),
    /// Ключ, вычисляемый во время выполнения (выражение типа `&'static str`).
    Dyn(TokenStream),
}

impl KeyExpr {
    /// Ключ дочернего узла с индексом `index`.
    fn child(&self, index: usize) -> Self {
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
    fn iteration(&self) -> Self {
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
    fn token(&self) -> TokenStream {
        match self {
            Self::Lit(key) => quote!(#key),
            Self::Dyn(expr) => expr.clone(),
        }
    }
}

/// Ключ ребёнка: на верхнем уровне — просто индекс, иначе `родитель.индекс`.
fn child_key(base: Option<&KeyExpr>, index: usize) -> KeyExpr {
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
fn static_key(element: &Element, fallback: &KeyExpr) -> KeyExpr {
    match attr_value(&element.attrs, "key") {
        Some(value) if !value.trim_start().starts_with(':') => {
            KeyExpr::Lit(value.trim().to_owned())
        }
        _ => fallback.clone(),
    }
}

/// Возвращает значение директивы `name` у элемента (с обрезкой пробелов).
fn element_directive<'a>(node: &'a Node, name: &str) -> Option<&'a str> {
    match node {
        Node::Element(element) => attr_value(&element.attrs, name),
        Node::Text(_) => None,
    }
}

/// Есть ли у элемента атрибут-флаг `name`.
fn has_attr(node: &Node, name: &str) -> bool {
    matches!(node, Node::Element(element) if attr_value(&element.attrs, name).is_some())
}

/// Разбирает условие `v-if`/`v-else-if` (с поддержкой ведущего `!`).
fn condition_token(condition: &str) -> syn::Result<TokenStream> {
    let condition = condition.trim();
    let (negate, path) = match condition.strip_prefix('!') {
        Some(rest) => (true, rest.trim()),
        None => (false, condition),
    };
    let expr = path_expr(path)?;
    Ok(if negate { quote!(!#expr) } else { expr })
}

/// Разбирает `v-for="item in items"`.
fn parse_for(spec: &str) -> syn::Result<(String, String)> {
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

impl Generator {
    /// Выражение пути с учётом локальных переменных `v-for`.
    ///
    /// Если первый сегмент совпадает с именем переменной цикла, путь строится
    /// от неё (`item`, `item.name()`), иначе — от `self` (`self.items()`).
    fn path_expr(&self, path: &str) -> syn::Result<TokenStream> {
        let segments = split_path(path)?;
        let is_local = segments
            .first()
            .is_some_and(|first| self.scopes.iter().any(|name| name == first));
        let mut expr = if is_local {
            let ident = to_ident(&segments[0])?;
            quote!(#ident)
        } else {
            quote!(self)
        };
        let start = if is_local { 1 } else { 0 };
        for segment in &segments[start..] {
            let ident = to_ident(segment)?;
            expr = quote!(#expr.#ident());
        }
        Ok(expr)
    }

    /// Выражение видимости элемента: `:visible="path"` (динамически) либо
    /// атрибут-флаг `visible`/`visible="false"` (статически).
    fn visible_token(&self, element: &Element) -> syn::Result<TokenStream> {
        if let Some(path) = attr_value(&element.attrs, ":visible") {
            let expr = self.path_expr(path)?;
            return Ok(quote!(#expr));
        }
        let visible = match attr_value(&element.attrs, "visible") {
            Some(value) if value.trim().is_empty() => true,
            Some(value) => parse_bool(value),
            None => true,
        };
        Ok(quote!(#visible))
    }

    /// Порядок отрисовки/хит-теста `z`: `:z="path"` (динамически) либо
    /// целочисленный атрибут `z` (статически, по умолчанию `0`).
    fn z_token(&self, element: &Element) -> syn::Result<TokenStream> {
        if let Some(path) = attr_value(&element.attrs, ":z") {
            return self.path_expr(path);
        }
        let Some(value) = attr_value(&element.attrs, "z") else {
            return Ok(quote!(0));
        };
        let number: i32 = value.trim().parse().map_err(|_| {
            syn::Error::new(
                Span::call_site(),
                format!(
                    "нечисловое значение z=\"{value}\" у <{}>: ожидается целое",
                    element.name
                ),
            )
        })?;
        Ok(quote!(#number))
    }

    /// Регистрирует пользовательскую проверку поля (`:validator="метод"`).
    fn record_field_validator(&mut self, element: &Element, key: &KeyExpr) -> syn::Result<()> {
        let Some(name) = attr_value(&element.attrs, ":validator") else {
            return Ok(());
        };
        let name = name.trim();
        if name.is_empty() {
            return Err(syn::Error::new(
                Span::call_site(),
                format!("пустое значение :validator у <{}>", element.name),
            ));
        }
        let key_string = match key {
            KeyExpr::Lit(value) => value.clone(),
            KeyExpr::Dyn(_) => {
                return Err(syn::Error::new(
                    Span::call_site(),
                    ":validator не поддерживается внутри v-for",
                ));
            }
        };
        let call = self.handler_call(name, Some(quote!(__value)))?;
        self.field_validators.push((key_string, call));
        Ok(())
    }

    /// Регистрирует пользовательскую проверку формы (`@validate="метод"`).
    fn record_form_validator(&mut self, element: &Element, key: &KeyExpr) -> syn::Result<()> {
        let Some(name) = attr_value(&element.attrs, "@validate") else {
            return Ok(());
        };
        let name = name.trim();
        if name.is_empty() {
            return Err(syn::Error::new(
                Span::call_site(),
                format!("пустое значение @validate у <{}>", element.name),
            ));
        }
        let key_string = match key {
            KeyExpr::Lit(value) => value.clone(),
            KeyExpr::Dyn(_) => {
                return Err(syn::Error::new(
                    Span::call_site(),
                    "@validate не поддерживается внутри v-for",
                ));
            }
        };
        let call = self.handler_call(name, None)?;
        self.form_validators.push((key_string, call));
        Ok(())
    }

    /// Собирает содержимое текстового узла с учётом локальных переменных.
    fn content_expr(&self, segments: &[Segment]) -> syn::Result<TokenStream> {
        match segments {
            [] => Ok(quote!(::std::string::String::new())),
            [Segment::Literal(literal)] => Ok(quote!(::std::string::String::from(#literal))),
            [Segment::Path(path)] => {
                let expr = self.path_expr(path)?;
                Ok(quote!(::std::string::ToString::to_string(&(#expr))))
            }
            _ => {
                let mut format_string = String::new();
                let mut args = Vec::new();
                for segment in segments {
                    match segment {
                        Segment::Literal(literal) => {
                            format_string.push_str(&escape_format(literal));
                        }
                        Segment::Path(path) => {
                            format_string.push_str("{}");
                            args.push(self.path_expr(path)?);
                        }
                    }
                }
                Ok(quote!(::std::format!(#format_string, #(#args),*)))
            }
        }
    }

    /// Обходит список детей, собирая `if`/`else if`/`else`-цепочки
    /// `v-if`/`v-else-if`/`v-else` и раскрывая `v-for`.
    fn gen_children(
        &mut self,
        children: &[Node],
        base: Option<&KeyExpr>,
    ) -> syn::Result<Vec<TokenStream>> {
        let mut stmts = Vec::new();
        let mut index = 0;
        while index < children.len() {
            let key = child_key(base, index);
            if let Some(condition) = element_directive(&children[index], "v-if") {
                let mut branches: Vec<(Option<&str>, &Node, KeyExpr)> = Vec::new();
                branches.push((Some(condition), &children[index], key));
                let mut next = index + 1;
                while next < children.len() {
                    let branch_key = child_key(base, next);
                    if let Some(condition) = element_directive(&children[next], "v-else-if") {
                        branches.push((Some(condition), &children[next], branch_key));
                    } else if has_attr(&children[next], "v-else") {
                        branches.push((None, &children[next], branch_key));
                        next += 1;
                        break;
                    } else {
                        break;
                    }
                    next += 1;
                }
                stmts.push(self.gen_chain(&branches)?);
                index = next;
            } else if element_directive(&children[index], "v-else-if").is_some()
                || has_attr(&children[index], "v-else")
            {
                return Err(syn::Error::new(
                    Span::call_site(),
                    "v-else-if/v-else должны следовать за v-if",
                ));
            } else {
                stmts.push(self.gen_push_node(&children[index], &key)?);
                index += 1;
            }
        }
        Ok(stmts)
    }

    /// Строит цепочку `if (cond) {…} else if (cond) {…} else {…}`.
    fn gen_chain(
        &mut self,
        branches: &[(Option<&str>, &Node, KeyExpr)],
    ) -> syn::Result<TokenStream> {
        let mut chain = TokenStream::new();
        for (position, (condition, node, key)) in branches.iter().enumerate() {
            let body = self.gen_push_node(node, key)?;
            match condition {
                Some(condition) => {
                    let condition = condition_token(condition)?;
                    if position == 0 {
                        chain.extend(quote! { if #condition { #body } });
                    } else {
                        chain.extend(quote! { else if #condition { #body } });
                    }
                }
                None => chain.extend(quote! { else { #body } }),
            }
        }
        Ok(chain)
    }

    /// Добавляет узел (вместе с `v-for`) в текущий `__children`.
    fn gen_push_node(&mut self, node: &Node, key: &KeyExpr) -> syn::Result<TokenStream> {
        match node {
            Node::Text(segments) => {
                let content = self.content_expr(segments)?;
                let key = key.token();
                Ok(quote! {
                    __children.push(::light_fw::vdom::VElement::leaf(
                        #key,
                        ::light_fw::vdom::NodeKind::Text { content: #content },
                    ));
                })
            }
            Node::Element(element) if element.name == "template" => {
                validate_attrs(&element.attrs, &["v-if", "v-else-if", "v-else", "v-for"])?;
                let pushes = self.gen_children(&element.children, Some(key))?;
                Ok(quote! { #(#pushes)* })
            }
            Node::Element(element) => {
                if let Some(spec) = attr_value(&element.attrs, "v-for") {
                    return self.gen_for_push(element, key, spec);
                }
                // Явный статический ключ (`key="..."`) переопределяет позиционный.
                let element = self.gen_element(element, &static_key(element, key))?;
                Ok(quote! { __children.push(#element); })
            }
        }
    }

    /// Раскрывает `v-for="item in items"` в цикл, повторяющий узел.
    fn gen_for_push(
        &mut self,
        element: &Element,
        key: &KeyExpr,
        spec: &str,
    ) -> syn::Result<TokenStream> {
        let (item, collection) = parse_for(spec)?;
        let collection = self.path_expr(&collection)?;
        let item_ident = to_ident(&item)?;
        let loop_key = key.iteration();
        self.scopes.push(item);
        let body = self.gen_element(element, &loop_key);
        self.scopes.pop();
        let body = body?;
        Ok(quote! {
            for (__for_index, #item_ident) in
                ::std::iter::IntoIterator::into_iter(#collection).enumerate()
            {
                __children.push(#body);
            }
        })
    }

    /// Имя выключенной Cargo-фичи, которой принадлежит элемент `name`.
    ///
    /// Используется для понятной ошибки компиляции: если фича выключена, элемент
    /// не должен молча превращаться в «неизвестный».
    fn disabled_element_feature(name: &str) -> Option<&'static str> {
        let feature = match name {
            "password" => "elements-input",
            "checkbox" => "elements-checkbox",
            "radio" => "elements-radio",
            "select" => "elements-select",
            "image" => "image-support",
            _ => return None,
        };
        let enabled = [
            ("elements-input", cfg!(feature = "elements-input")),
            ("elements-checkbox", cfg!(feature = "elements-checkbox")),
            ("elements-radio", cfg!(feature = "elements-radio")),
            ("elements-select", cfg!(feature = "elements-select")),
            ("image-support", cfg!(feature = "image-support")),
        ]
        .into_iter()
        .find(|(name, _)| *name == feature)
        .is_none_or(|(_, enabled)| enabled);
        (!enabled).then_some(feature)
    }

    /// Строит выражение `VElement` для элемента (без учёта `v-if`).
    fn gen_element(&mut self, element: &Element, key: &KeyExpr) -> syn::Result<TokenStream> {
        validate_a11y_attrs(element)?;
        if let Some(feature) = Self::disabled_element_feature(&element.name) {
            return Err(syn::Error::new(
                Span::call_site(),
                format!(
                    "элемент <{}> недоступен: Cargo-фича `{}` выключена у крейта `light-fw`\n\
                     help: включите её в зависимости, например \
                     `light-fw = {{ features = [\"{}\"] }}`",
                    element.name, feature, feature
                ),
            ));
        }
        match element.name.as_str() {
            "div" => self.gen_container(element, key, None),
            "span" => self.gen_container(element, key, None),
            "h1" => self.gen_container(element, key, Some(32.0)),
            "h2" => self.gen_container(element, key, Some(24.0)),
            "h3" => self.gen_container(element, key, Some(20.0)),
            "h4" => self.gen_container(element, key, Some(18.0)),
            "h5" => self.gen_container(element, key, Some(16.0)),
            "button" => self.gen_button(element, key),
            "titlebar" => self.gen_titlebar(element, key),
            "text" => self.gen_text_field(element, key, false),
            "password" => self.gen_text_field(element, key, true),
            "checkbox" => self.gen_checkbox(element, key),
            "radio" => self.gen_radio(element, key),
            "select" => self.gen_select(element, key),
            "image" => self.gen_image(element, key),
            other => Err(syn::Error::new(
                Span::call_site(),
                format!(
                    "неизвестный элемент <{other}>; поддерживаются: \
                     <div>, <span>, <h1>…<h5>, <text>, <password>, <checkbox>, <radio>, \
                     <select>, <option>, <button>, <titlebar>, <image>, <include>, <template>"
                ),
            )),
        }
    }

    /// Ветви обработчиков событий элемента; регистрируются в `arms`.
    fn push_events(&mut self, element: &Element, key: &TokenStream) -> syn::Result<()> {
        let branches = self.event_branches(element)?;
        if !branches.is_empty() {
            self.arms.push(quote! {
                if __key == #key {
                    #(#branches)*
                    __handled = true;
                }
            });
        }
        // Запоминаем @click для runtime-диспетчеризации (фича `runtime-templates`):
        // имя обработчика (без скобок вызова) сопоставляется в `invoke_handler`.
        // Ключи не дедуплицируются: у двух узлов с одним и тем же именем — один
        // рабочий обработчик, что нужно для runtime-дерева, где имя = ключ узла.
        if let Some(handler) = attr_value(&element.attrs, "@click")
            && is_plain_click_handler(handler)
        {
            let name = normalize_click_name(handler);
            let call = self.handler_call(handler, None)?;
            self.click_handlers.push((key.clone(), (call, name)));
        }
        Ok(())
    }

    /// Имя обработчика `@click` для runtime-диспетчеризации по имени.
    ///
    /// Убирает скобки вызова (`"increment()"` → `"increment"`), пробельные
    /// символы вокруг и ведущее `this.` — в runtime-дереве это имя становится
    /// ключом узла, а статический код макроса дотягивается до него через
    /// [`Component::invoke_handler`](crate::component::Component).
    /// Ветви `if`-проверок для всех событийных директив элемента.
    fn event_branches(&self, element: &Element) -> syn::Result<Vec<TokenStream>> {
        let mut branches = Vec::new();
        let simple = [
            ("@click", "Click"),
            ("@dblclick", "Dblclick"),
            ("@mouseenter", "Mouseenter"),
            ("@mouseleave", "Mouseleave"),
        ];
        for (attr_name, variant) in simple {
            if let Some(handler) = attr_value(&element.attrs, attr_name) {
                let call = self.handler_call(handler, None)?;
                let variant = Ident::new(variant, Span::call_site());
                // `handler_call` возвращает:
                //  • полновесный вызов для явных аргументов: `self.open('x')`
                //    или для `::`-выражений: `Dom::window().map(...)`
                //  • метод-ссылку для имён и пустых скобок: `self.increment`
                let trimmed = handler.trim();
                let has_non_empty_args = trimmed
                    .find('(')
                    .and_then(|i| {
                        let rest = &trimmed[i..];
                        let close = rest.rfind(')')?;
                        let inner = rest[1..close].trim();
                        (!inner.is_empty()).then_some(())
                    })
                    .is_some();
                let is_full_call = has_non_empty_args || trimmed.contains("::");
                let body = if is_full_call {
                    quote! { #call }
                } else {
                    quote! { #call() }
                };
                branches.push(quote! {
                    if ::std::matches!(&__event, ::light_fw::vdom::Event::#variant) {
                        #body;
                        return true;
                    }
                });
            }
        }
        for (attr_name, variant) in [("@mousedown", "Mousedown"), ("@mouseup", "Mouseup")] {
            if let Some(handler) = attr_value(&element.attrs, attr_name) {
                let call = self.handler_call(handler, Some(quote!(*__button)))?;
                let variant = Ident::new(variant, Span::call_site());
                // `handler_call` с аргументом события уже вернул полновесный вызов.
                branches.push(quote! {
                    if let ::light_fw::vdom::Event::#variant(__button) = &__event {
                        #call;
                        return true;
                    }
                });
            }
        }
        if let Some(handler) = attr_value(&element.attrs, "@mousemove") {
            let call = self.handler_call(handler, Some(quote!(*__x, *__y)))?;
            branches.push(quote! {
                if let ::light_fw::vdom::Event::Mousemove { x: __x, y: __y } = &__event {
                    #call;
                    return true;
                }
            });
        }
        for (attr_name, variant) in [("@keydown", "Keydown"), ("@keyup", "Keyup")] {
            if let Some(handler) = attr_value(&element.attrs, attr_name) {
                let call = self.handler_call(handler, Some(quote!(__info.clone())))?;
                let variant = Ident::new(variant, Span::call_site());
                branches.push(quote! {
                    if let ::light_fw::vdom::Event::#variant(__info) = &__event {
                        #call;
                        return true;
                    }
                });
            }
        }
        Ok(branches)
    }

    /// Вызов обработчика.
    ///
    /// Если значение содержит `::` (например
    /// `Dom::find(|node| node.attr("id") == "id123").first().close()`), оно
    /// разбирается как произвольное Rust-выражение и подставляется как есть.
    /// Иначе значение трактуется как путь метода компонента (возможно со
    /// строковыми аргументами, например `open('settings')`), а `args` —
    /// аргументы, внедряемые из события. Явные аргументы вызова имеют приоритет.
    fn handler_call(&self, handler: &str, args: Option<TokenStream>) -> syn::Result<TokenStream> {
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

    /// Токены метаданных `VElement`: имя HTML-тега и атрибуты (кроме директив
    /// `v-*` и обработчиков `@*`). Динамические привязки `:attr="путь"`
    /// записываются под именем без двоеточия со строковым значением.
    fn node_meta(&self, element: &Element) -> syn::Result<TokenStream> {
        let tag = &element.name;
        let mut pairs = Vec::new();
        for attr in &element.attrs {
            let name = attr.name.as_str();
            if name.starts_with('@')
                || name.starts_with("v-")
                || name == ":validator"
                || name == ":src"
                || name == "required-indicator"
                || name == "key"
            {
                continue;
            }
            if let Some(base) = name.strip_prefix(':') {
                let base = base.to_owned();
                let value = self.path_expr(&attr.value)?;
                pairs.push(quote! {
                    (#base, ::std::string::ToString::to_string(&(#value)))
                });
            } else {
                let name = name.to_owned();
                let value = &attr.value;
                pairs.push(quote! {
                    (#name, ::std::string::String::from(#value))
                });
            }
        }
        let attrs = if pairs.is_empty() {
            quote!()
        } else {
            quote!(.with_attrs(::std::vec![#(#pairs),*]))
        };
        Ok(quote!(.with_tag(#tag)#attrs))
    }

    /// Строит контейнер (`div`/`span`/`h1`…`h5`): вертикальный по умолчанию,
    /// `direction="horizontal"` — горизонтальный.
    fn gen_container(
        &mut self,
        element: &Element,
        key: &KeyExpr,
        default_font: Option<f32>,
    ) -> syn::Result<TokenStream> {
        self.record_form_validator(element, key)?;
        let meta = self.node_meta(element)?;
        let horizontal = match attr_value(&element.attrs, "direction").map(str::trim) {
            None | Some("vertical") => false,
            Some("horizontal") => true,
            Some(other) => {
                return Err(syn::Error::new(
                    Span::call_site(),
                    format!(
                        "недопустимое значение direction=\"{other}\" у <{}>; \
                         ожидается \"vertical\" или \"horizontal\"",
                        element.name
                    ),
                ));
            }
        };
        let kind = if horizontal {
            quote!(::light_fw::vdom::NodeKind::Row)
        } else {
            quote!(::light_fw::vdom::NodeKind::Column)
        };
        let pushes = self.gen_children(&element.children, Some(key))?;
        let style = style_token(self, element, default_font, self.visible_token(element)?)?;
        let key = key.token();
        self.push_events(element, &key)?;
        Ok(quote! {{
            let mut __children = ::std::vec::Vec::new();
            #(#pushes)*
            ::light_fw::vdom::VElement::container(#key, #kind, __children)
                .with_style(#style)
                #meta
        }})
    }
    fn gen_titlebar(&mut self, element: &Element, key: &KeyExpr) -> syn::Result<TokenStream> {
        validate_attrs(&element.attrs, &["v-if", "v-else-if", "v-else", "v-for"])?;
        let meta = self.node_meta(element)?;
        let pushes = self.gen_children(&element.children, Some(key))?;
        let style = style_token(self, element, None, self.visible_token(element)?)?;
        let key = key.token();
        self.push_events(element, &key)?;
        Ok(quote! {{
            let mut __children = ::std::vec::Vec::new();
            #(#pushes)*
            ::light_fw::vdom::VElement::container(#key, ::light_fw::vdom::NodeKind::TitleBar, __children)
                .with_style(#style)
                #meta
        }})
    }

    /// Строит `<image>`: статический `src` встраивается в бинарник, а
    /// динамический `:src` (значение `Option<ImageData>`) заменяет его во время
    /// выполнения.
    fn gen_image(&mut self, element: &Element, key: &KeyExpr) -> syn::Result<TokenStream> {
        if !element.children.is_empty() {
            return Err(syn::Error::new(
                Span::call_site(),
                "<image> — листовой элемент и не может содержать дочерние элементы",
            ));
        }
        let src = attr_value(&element.attrs, "src")
            .map(str::trim)
            .filter(|src| !src.is_empty())
            .ok_or_else(|| {
                syn::Error::new(
                    Span::call_site(),
                    "у <image> обязателен атрибут src=\"путь/к/файлу.png\"",
                )
            })?;
        let dynamic_src = attr_value(&element.attrs, ":src");
        let path = asset_path(src)?;
        let (width, height) = png_dimensions(&path)?;
        let path_literal = path_literal(&path);
        let alt = attr_value(&element.attrs, "alt").unwrap_or_default();
        let meta = self.node_meta(element)?;
        let style = style_token(self, element, None, self.visible_token(element)?)?;
        let key = key.token();
        self.push_events(element, &key)?;
        let fallback = quote! {
            ::light_fw::vdom::VElement::container(
                #key,
                ::light_fw::vdom::NodeKind::Image {
                    src: #src,
                    data: ::std::include_bytes!(#path_literal),
                    width: #width,
                    height: #height,
                    alt: ::std::string::String::from(#alt),
                },
                ::std::vec::Vec::new(),
            )
        };
        let node = if let Some(dynamic) = dynamic_src {
            let expr = self.path_expr(dynamic)?;
            quote! {
                match #expr {
                    ::std::option::Option::Some(__image) => {
                        ::light_fw::vdom::VElement::container(
                            #key,
                            ::light_fw::vdom::NodeKind::RuntimeImage {
                                image: __image,
                                alt: ::std::string::String::from(#alt),
                            },
                            ::std::vec::Vec::new(),
                        )
                    }
                    ::std::option::Option::None => #fallback,
                }
            }
        } else {
            fallback
        };
        Ok(quote! {{
            (#node)
                .with_style(#style)
                #meta
        }})
    }

    fn gen_button(&mut self, element: &Element, key: &KeyExpr) -> syn::Result<TokenStream> {
        validate_attrs(
            &element.attrs,
            &[
                "@click",
                ":disabled",
                "type",
                "v-if",
                "v-else-if",
                "v-else",
                "v-for",
            ],
        )?;
        let disabled = match attr_value(&element.attrs, ":disabled") {
            Some(disabled) => {
                let expr = self.path_expr(disabled)?;
                quote!(#expr)
            }
            None => quote!(false),
        };
        let button_type = match dynamic_value(self, element, "type")? {
            Some(value) => quote!(::light_fw::vdom::parse_button_type(&#value)),
            None => match attr_value(&element.attrs, "type").map(str::trim) {
                None | Some("") | Some("submit") => quote!(::light_fw::vdom::ButtonType::Submit),
                Some("button") => quote!(::light_fw::vdom::ButtonType::Button),
                Some("reset") => quote!(::light_fw::vdom::ButtonType::Reset),
                Some(other) => {
                    return Err(syn::Error::new(
                        Span::call_site(),
                        format!(
                            "invalid type=\"{other}\" on <button>: expected \"submit\", \"button\" \
                             or \"reset\""
                        ),
                    ));
                }
            },
        };

        let pushes = self.gen_children(&element.children, Some(key))?;
        let style = style_token(self, element, None, self.visible_token(element)?)?;
        let meta = self.node_meta(element)?;
        let key = key.token();
        self.push_events(element, &key)?;
        Ok(quote! {{
            let mut __children = ::std::vec::Vec::new();
            #(#pushes)*
            ::light_fw::vdom::VElement::container(
                #key,
                ::light_fw::vdom::NodeKind::Button {
                    disabled: #disabled,
                    button_type: #button_type,
                },
                __children,
            )
            .with_style(#style)
            #meta
        }})
    }

    fn gen_text_field(
        &mut self,
        element: &Element,
        key: &KeyExpr,
        masked: bool,
    ) -> syn::Result<TokenStream> {
        validate_attrs(
            &element.attrs,
            &[
                "v-model",
                "placeholder",
                ":disabled",
                "textarea",
                "resize",
                "v-if",
                "v-else-if",
                "v-else",
                "v-for",
            ],
        )?;
        if !element.children.is_empty() {
            return Err(syn::Error::new(
                Span::call_site(),
                format!(
                    "<{}> не может содержать вложенных элементов или текст",
                    element.name
                ),
            ));
        }
        self.record_field_validator(element, key)?;
        let model = attr_value(&element.attrs, "v-model").ok_or_else(|| {
            syn::Error::new(
                Span::call_site(),
                format!(
                    "<{}> требует двустороннюю привязку: v-model=\"свойство\"",
                    element.name
                ),
            )
        })?;
        let read = self.path_expr(model)?;
        let write = setter_expr(model, &quote!(__value.clone()))?;
        let placeholder = match dynamic_value(self, element, "placeholder")? {
            Some(value) => value,
            None => {
                let value = attr_value(&element.attrs, "placeholder").unwrap_or("");
                quote!(::std::string::String::from(#value))
            }
        };
        let disabled = match attr_value(&element.attrs, ":disabled") {
            Some(disabled) => {
                let expr = self.path_expr(disabled)?;
                quote!(#expr)
            }
            None => quote!(false),
        };
        let multiline = if masked {
            quote!(false)
        } else {
            match dynamic_value(self, element, "textarea")? {
                Some(value) => quote!(::light_fw::vdom::parse_bool(&#value)),
                None => {
                    let value = match attr_value(&element.attrs, "textarea") {
                        None => false,
                        Some("") => true,
                        Some(value) => parse_bool(value),
                    };
                    quote!(#value)
                }
            }
        };
        let resize = if masked {
            quote!(::light_fw::vdom::Resize::None)
        } else {
            match dynamic_value(self, element, "resize")? {
                Some(value) => quote!(::light_fw::vdom::parse_resize(&#value)),
                None => match attr_value(&element.attrs, "resize").map(str::trim) {
                    None | Some("none") => quote!(::light_fw::vdom::Resize::None),
                    Some("horizontal") => quote!(::light_fw::vdom::Resize::Horizontal),
                    Some("vertical") => quote!(::light_fw::vdom::Resize::Vertical),
                    Some("both") => quote!(::light_fw::vdom::Resize::Both),
                    Some(other) => {
                        return Err(syn::Error::new(
                            Span::call_site(),
                            format!(
                                "invalid resize=\"{other}\" on <{}>: expected \"horizontal\", \"vertical\" or \"both\"",
                                element.name
                            ),
                        ));
                    }
                },
            }
        };

        let key = key.token();
        let mut branches = self.event_branches(element)?;
        branches.push(quote! {
            if let ::light_fw::vdom::Event::TextChange(__value) = &__event {
                #write; return true;
            }
        });
        self.arms.push(quote! {
            if __key == #key { #(#branches)* }
        });

        let style = style_token(self, element, None, self.visible_token(element)?)?;
        let meta = self.node_meta(element)?;
        Ok(quote! {
            ::light_fw::vdom::VElement::leaf(
                #key,
                ::light_fw::vdom::NodeKind::TextField {
                    value: ::std::string::ToString::to_string(&(#read)),
                    placeholder: #placeholder,
                    disabled: #disabled,
                    multiline: #multiline,
                    resize: #resize,
                    masked: #masked,
                },
            )
            .with_style(#style)
            #meta
        })
    }

    /// Выражение флага `:disabled` (по умолчанию `false`).
    fn disabled_token(&self, element: &Element) -> syn::Result<TokenStream> {
        Ok(match attr_value(&element.attrs, ":disabled") {
            Some(disabled) => self.path_expr(disabled)?,
            None => quote!(false),
        })
    }

    /// Строит флажок (`<checkbox>`) с `v-model` булева свойства.
    fn gen_checkbox(&mut self, element: &Element, key: &KeyExpr) -> syn::Result<TokenStream> {
        validate_attrs(
            &element.attrs,
            &[
                "v-model",
                ":disabled",
                "v-if",
                "v-else-if",
                "v-else",
                "v-for",
            ],
        )?;
        let model = attr_value(&element.attrs, "v-model").ok_or_else(|| {
            syn::Error::new(
                Span::call_site(),
                "<checkbox> требует двустороннюю привязку: v-model=\"свойство\"",
            )
        })?;
        self.record_field_validator(element, key)?;
        let read = self.path_expr(model)?;
        let write = setter_expr(model, &quote!(!(#read)))?;
        let label = self.content_expr(&text_segments(&element.children, "checkbox")?)?;
        let disabled = self.disabled_token(element)?;

        let key = key.token();
        let mut branches = self.event_branches(element)?;
        branches.push(quote! {
            if let ::light_fw::vdom::Event::Click = &__event {
                #write; return true;
            }
        });
        self.arms.push(quote! {
            if __key == #key { #(#branches)* }
        });

        // Регистрация v-model для runtime-writeback (dispatch_vmodel).
        // Записываем имя поля без вложенных путей (только простые идентификаторы),
        // потому что runtime-дерево поддерживает только `{{ поле }}` без точек.
        let model_root = model.split('.').next().unwrap_or(model).to_string();
        self.vmodel_bindings.push((model_root, "bool".to_string(), None));

        let style = style_token(self, element, None, self.visible_token(element)?)?;
        let meta = self.node_meta(element)?;
        Ok(quote! {
            ::light_fw::vdom::VElement::leaf(
                #key,
                ::light_fw::vdom::NodeKind::Checkbox {
                    label: #label,
                    checked: #read,
                    disabled: #disabled,
                },
            )
            .with_style(#style)
            #meta
        })
    }

    /// Строит переключатель (`<radio>`) с общим для группы `v-model`.
    fn gen_radio(&mut self, element: &Element, key: &KeyExpr) -> syn::Result<TokenStream> {
        validate_attrs(
            &element.attrs,
            &[
                "v-model",
                "value",
                ":disabled",
                "v-if",
                "v-else-if",
                "v-else",
                "v-for",
            ],
        )?;
        let model = attr_value(&element.attrs, "v-model").ok_or_else(|| {
            syn::Error::new(
                Span::call_site(),
                "<radio> требует двустороннюю привязку: v-model=\"свойство\"",
            )
        })?;
        self.record_field_validator(element, key)?;
        let (value, compare) = match dynamic_value(self, element, "value")? {
            Some(value) => (value.clone(), value),
            None => {
                let literal = attr_value(&element.attrs, "value").ok_or_else(|| {
                    syn::Error::new(
                        Span::call_site(),
                        "<radio> требует атрибут value=\"значение\"",
                    )
                })?;
                (
                    quote!(::std::string::String::from(#literal)),
                    quote!(#literal),
                )
            }
        };
        let read = self.path_expr(model)?;
        let write = setter_expr(model, &quote!(#value))?;
        let label = self.content_expr(&text_segments(&element.children, "radio")?)?;
        let disabled = self.disabled_token(element)?;

        let key = key.token();
        let mut branches = self.event_branches(element)?;
        branches.push(quote! {
            if let ::light_fw::vdom::Event::Click = &__event {
                #write; return true;
            }
        });
        self.arms.push(quote! {
            if __key == #key { #(#branches)* }
        });

        // Регистрация v-model для runtime-writeback (dispatch_vmodel).
        let model_root = model.split('.').next().unwrap_or(model).to_string();
        let radio_value = attr_value(&element.attrs, "value")
            .filter(|v| !v.starts_with(':'))
            .map(str::to_string);
        self.vmodel_bindings.push((model_root, "string".to_string(), radio_value));

        let style = style_token(self, element, None, self.visible_token(element)?)?;
        let meta = self.node_meta(element)?;
        Ok(quote! {
            ::light_fw::vdom::VElement::leaf(
                #key,
                ::light_fw::vdom::NodeKind::Radio {
                    label: #label,
                    value: #value,
                    selected: #read == #compare,
                    disabled: #disabled,
                },
            )
            .with_style(#style)
            #meta
        })
    }

    /// Строит выпадающий список (`<select>` с `<option>`).
    fn gen_select(&mut self, element: &Element, key: &KeyExpr) -> syn::Result<TokenStream> {
        validate_attrs(
            &element.attrs,
            &[
                "v-model",
                ":disabled",
                "v-if",
                "v-else-if",
                "v-else",
                "v-for",
            ],
        )?;
        let model = attr_value(&element.attrs, "v-model").ok_or_else(|| {
            syn::Error::new(
                Span::call_site(),
                "<select> требует двустороннюю привязку: v-model=\"свойство\"",
            )
        })?;
        self.record_field_validator(element, key)?;
        let read = self.path_expr(model)?;
        let write = setter_expr(model, &quote!(__value.clone()))?;
        let disabled = self.disabled_token(element)?;

        let mut options = Vec::new();
        for child in &element.children {
            let Node::Element(option) = child else {
                return Err(syn::Error::new(
                    Span::call_site(),
                    "<select> может содержать только элементы <option>",
                ));
            };
            if option.name != "option" {
                return Err(syn::Error::new(
                    Span::call_site(),
                    format!(
                        "<select> может содержать только <option>, найден <{}>",
                        option.name
                    ),
                ));
            }
            let label = self.content_expr(&text_segments(&option.children, "option")?)?;
            let value = match dynamic_value(self, option, "value")? {
                Some(value) => value,
                None => match attr_value(&option.attrs, "value") {
                    Some(value) => quote!(::std::string::String::from(#value)),
                    None => label.clone(),
                },
            };
            options.push(quote! {
                ::light_fw::vdom::SelectOption {
                    label: (#label).clone(),
                    value: #value,
                }
            });
        }
        let options = quote!(::std::vec![#(#options),*]);

        let key = key.token();
        let mut branches = self.event_branches(element)?;
        branches.push(quote! {
            if let ::light_fw::vdom::Event::TextChange(__value) = &__event {
                #write; return true;
            }
        });
        self.arms.push(quote! {
            if __key == #key { #(#branches)* }
        });

        // Регистрация v-model для runtime-writeback (dispatch_vmodel).
        let model_root = model.split('.').next().unwrap_or(model).to_string();
        self.vmodel_bindings.push((model_root, "string".to_string(), None));

        let style = style_token(self, element, None, self.visible_token(element)?)?;
        let meta = self.node_meta(element)?;
        Ok(quote! {
            ::light_fw::vdom::VElement::leaf(
                #key,
                ::light_fw::vdom::NodeKind::Select {
                    value: ::std::string::ToString::to_string(&(#read)),
                    options: #options,
                    disabled: #disabled,
                },
            )
            .with_style(#style)
            #meta
        })
    }
}

/// РЎРѕР±РёСЂР°РµС‚ РЎРѓР С•Р Т‘Р ВµРЎР‚Р В¶Р С‘Р СР С•Р Вµ С‚РµРєСЃС‚РѕРІРѕРіРѕ СѓР·Р»Р° РІ РІС‹СЂР°Р¶РµРЅРёРµ С‚РёРїР° `String`.
fn escape_format(literal: &str) -> String {
    literal.replace('{', "{{").replace('}', "}}")
}

/// `state.count` в†’ `self.state().count()`
fn path_expr(path: &str) -> syn::Result<TokenStream> {
    let segments = split_path(path)?;
    let mut expr = quote!(self);
    for segment in segments {
        let ident = to_ident(&segment)?;
        expr = quote!(#expr.#ident());
    }
    Ok(expr)
}

/// Разбирает атрибут размера (`width`/`height`): число в пикселях
/// (`120`, `120px`) или проценты от родителя (`50%`).
fn parse_size_attr(element: &Element, name: &str) -> syn::Result<Option<TokenStream>> {
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
fn style_token(
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
fn flex_attr(
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
fn parse_flex_number(element: &Element, name: &str, value: &str) -> syn::Result<f32> {
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
fn optional_size_token(element: &Element, name: &str) -> syn::Result<TokenStream> {
    Ok(match parse_size_attr(element, name)? {
        Some(token) => quote!(::std::option::Option::Some(#token)),
        None => quote!(::std::option::Option::None),
    })
}

/// Разбирает атрибут выравнивания (`position-x`/`position-y`).
fn align_token(
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
fn offset_token(element: &Element, name: &str) -> syn::Result<TokenStream> {
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
fn dynamic_value(
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
fn size_attr(
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
fn optional_size_attr(
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
fn align_attr(
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
fn offset_attr(generator: &Generator, element: &Element, name: &str) -> syn::Result<TokenStream> {
    if let Some(value) = dynamic_value(generator, element, name)? {
        return Ok(quote!(::light_fw::vdom::parse_offset(&#value)));
    }
    offset_token(element, name)
}

/// Участие в потоке (`position`): `flow`/`static`/`relative` или `absolute`.
fn position_attr(generator: &Generator, element: &Element) -> syn::Result<TokenStream> {
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
fn overflow_attr(
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

/// Перенос текста (`white-space`): `normal`/`wrap` или `nowrap`.
fn white_space_attr(generator: &Generator, element: &Element) -> syn::Result<TokenStream> {
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
fn edges_attr(generator: &Generator, element: &Element, base: &str) -> syn::Result<TokenStream> {
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
fn px_attr(
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
fn color_attr(generator: &Generator, element: &Element, name: &str) -> syn::Result<TokenStream> {
    if let Some(value) = dynamic_value(generator, element, name)? {
        return Ok(quote!(::light_fw::vdom::parse_color(&#value)));
    }
    Ok(color_token(parse_color_attr(element, name)?))
}

/// Выражение флага `required` с учётом динамической привязки `:required`.
fn required_attr(generator: &Generator, element: &Element) -> syn::Result<TokenStream> {
    if let Some(value) = dynamic_value(generator, element, "required")? {
        return Ok(quote!(::light_fw::vdom::parse_bool(&#value)));
    }
    Ok(required_token(element))
}

/// Выражение индикатора обязательности с учётом динамической привязки
/// `:required-indicator`.
fn required_indicator_attr(generator: &Generator, element: &Element) -> syn::Result<TokenStream> {
    if let Some(value) = dynamic_value(generator, element, "required-indicator")? {
        return Ok(quote!(::light_fw::vdom::parse_required_indicator(&#value)));
    }
    required_indicator_token(element)
}

/// Необязательный цвет как `Option<u32>`.
fn color_token(value: Option<u32>) -> TokenStream {
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
fn edges_token(element: &Element, base: &str) -> syn::Result<TokenStream> {
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
fn parse_px_attr(element: &Element, name: &str) -> syn::Result<Option<f32>> {
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
fn parse_color_attr(element: &Element, name: &str) -> syn::Result<Option<u32>> {
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
fn required_token(element: &Element) -> TokenStream {
    match attr_value(&element.attrs, "required").map(str::trim) {
        None | Some("false") | Some("0") | Some("no") => quote!(false),
        Some(_) => quote!(true),
    }
}

/// Выражение индикатора обязательности (`required-indicator`).
///
/// `none` — без индикатора, `*` (по умолчанию) — звёздочка, цвет
/// (`#RRGGBB`/`#RRGGBBAA`) — акцентная полоса заданного цвета.
fn required_indicator_token(element: &Element) -> syn::Result<TokenStream> {
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
fn setter_expr(path: &str, value: &TokenStream) -> syn::Result<TokenStream> {
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

fn split_path(path: &str) -> syn::Result<Vec<String>> {
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
fn parse_call_args(inner: &str) -> syn::Result<Vec<TokenStream>> {
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

fn is_plain_ident(text: &str) -> bool {
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
fn is_plain_click_handler(handler: &str) -> bool {
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
fn normalize_click_name(handler: &str) -> String {
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
fn collect_interpolations(nodes: &[Node]) -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::new();
    for node in nodes {
        match node {
            Node::Element(element) => {
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

/// Все имена переменных `v-for` в шаблоне. Эти идентификаторы — локальные
/// переменные цикла, а не методы/поля компонента, поэтому их нужно исключить
/// из веток `eval_field`.
fn collect_v_for_items(nodes: &[Node]) -> std::collections::HashSet<String> {
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

/// Является ли тип поля структуры примитивным, который всегда реализует
/// `serde::Serialize` и безопасно сериализуется «через геттер» в снимке
/// состояния (ТЗ 0.13-D-fix, задача 2/3): числа (`i8…i128`, `u8…u128`,
/// `usize`, `isize`, `f32`, `f64`), `bool`, `char`, `str`, `String`,
/// `Cow`, а также их `Option`-обёртки (`Option<i64>` и т. п.).
///
/// Вложенные прокси-компоненты, `Vec` и прочие сложные типы не попадают
/// сюда: их `inspect_state` показывает как `null`, что не требует
/// `#[derive(Serialize)]` нигде и позволяет собрать `--all-features`.
fn is_primitive_serde(ty: &syn::Type) -> bool {
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

fn to_ident(segment: &str) -> syn::Result<Ident> {
    syn::parse_str::<Ident>(segment).map_err(|_| {
        syn::Error::new(
            Span::call_site(),
            format!("Р Р…Р ВµР Т‘Р С•Р С—РЎС“РЎРѓРЎвЂљР С‘Р СР С•Р Вµ Р С‘Р СРЎРЏ `{segment}` РІ РїСѓС‚Рё С€Р°Р±Р»РѕРЅР°"),
        )
    })
}

/// Р вЂќР ВµРЎвЂљР ВµРЎР‚Р СР С‘Р Р…Р С‘РЎР‚РЎС“Р ВµРЎвЂљ РЎРѓР С•Р Т‘Р ВµРЎР‚Р В¶Р С‘Р СР С•Р Вµ `<text>`, `<button>`: С‚РѕР»СЊРєРѕ С‚РµРєСЃС‚ Рё РёРЅС‚РµСЂРїРѕР»СЏС†РёСЏ.
fn text_segments(children: &[Node], element: &str) -> syn::Result<Vec<Segment>> {
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

/// Допустимые статические значения атрибута `cursor` (совпадают с
/// [`light_fw::vdom::Cursor::from_name`]).
const CURSOR_VALUES: &[&str] = &[
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

/// Проверяет статические атрибуты доступности. `tabindex` должен быть целым
/// числом, `cursor` — одним из известных имён; динамические привязки
/// `:tabindex="…"` и `:cursor="…"` проверяются во время выполнения.
fn validate_a11y_attrs(element: &Element) -> syn::Result<()> {
    for attr in &element.attrs {
        if attr.name == "tabindex" && attr.value.trim().parse::<i64>().is_err() {
            return Err(syn::Error::new(
                Span::call_site(),
                format!(
                    "недопустимое значение tabindex=\"{}\" у <{}>; ожидается целое число",
                    attr.value, element.name
                ),
            ));
        }
        if attr.name == "cursor" && !CURSOR_VALUES.contains(&attr.value.trim()) {
            return Err(syn::Error::new(
                Span::call_site(),
                format!(
                    "недопустимое значение cursor=\"{}\" у <{}>; ожидается одно из: {}",
                    attr.value,
                    element.name,
                    CURSOR_VALUES.join(", ")
                ),
            ));
        }
    }
    Ok(())
}

fn validate_attrs(attrs: &[Attr], known: &[&str]) -> syn::Result<()> {
    for attr in attrs {
        if !known.contains(&attr.name.as_str()) {
            continue;
        }
        if attrs.iter().filter(|other| other.name == attr.name).count() > 1 {
            return Err(syn::Error::new(
                Span::call_site(),
                format!("attribute `{}` is specified more than once", attr.name),
            ));
        }
    }
    Ok(())
}

fn attr_value<'a>(attrs: &'a [Attr], name: &str) -> Option<&'a str> {
    attrs
        .iter()
        .find(|attr| attr.name == name)
        .map(|attr| attr.value.as_str())
}

/// Кодирует идентификатор окна в hex — для имени символа проверки уникальности.
fn hex_id(id: &str) -> String {
    let mut out = String::with_capacity(id.len() * 2);
    for byte in id.bytes() {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}
/// Р Р°Р·РґРµР»СЏРµС‚ РєРѕСЂРЅРµРІРѕР№ <window> Рё РЎРѓР С•Р Т‘Р ВµРЎР‚Р В¶Р С‘Р СР С•Р Вµ.
fn separate_window(nodes: Vec<Node>) -> syn::Result<(Option<Element>, Vec<Node>)> {
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
struct WindowSpec {
    width: Option<u32>,
    height: Option<u32>,
    background: Option<String>,
    transparent: Option<String>,
    resizable: Option<String>,
    min_width: Option<u32>,
    min_height: Option<u32>,
    max_width: Option<u32>,
    max_height: Option<u32>,
    modal: Option<String>,
    visible: Option<String>,
    id: Option<String>,
    parent: Option<String>,
    icon: Option<String>,
}

fn parse_window(element: &Element) -> syn::Result<WindowSpec> {
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
fn asset_path(value: &str) -> syn::Result<PathBuf> {
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
fn path_literal(path: &Path) -> LitStr {
    LitStr::new(
        &path.to_string_lossy().replace('\\', "/"),
        Span::call_site(),
    )
}

/// Проверяет сигнатуру PNG и читает размеры из первого блока IHDR.
fn png_dimensions(path: &Path) -> syn::Result<(u32, u32)> {
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
fn parse_hex_color(value: &str) -> syn::Result<u32> {
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

fn parse_bool(value: &str) -> bool {
    matches!(value.trim(), "true" | "1" | "yes" | "on")
}

fn window_tokens(spec: &WindowSpec) -> syn::Result<TokenStream> {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn element(name: &str, attrs: &[(&str, &str)]) -> Element {
        Element {
            name: name.to_string(),
            attrs: attrs
                .iter()
                .map(|(name, value)| Attr {
                    name: name.to_string(),
                    value: value.to_string(),
                })
                .collect(),
            children: Vec::new(),
        }
    }

    #[test]
    fn tabindex_accepts_integers() {
        for value in ["0", "-1", "3", " 2 "] {
            let element = element("div", &[("tabindex", value)]);
            assert!(
                validate_a11y_attrs(&element).is_ok(),
                "значение tabindex=\"{value}\" должно приниматься"
            );
        }
    }

    #[test]
    fn tabindex_rejects_non_numeric_values() {
        for value in ["abc", "1.5", "", "0x10", "first"] {
            let element = element("button", &[("tabindex", value)]);
            let error = validate_a11y_attrs(&element).expect_err("должна быть ошибка");
            let message = error.to_string();
            assert!(
                message.contains("tabindex"),
                "сообщение должно упоминать tabindex, получено: {message}"
            );
        }
    }

    #[test]
    fn dynamic_tabindex_is_not_checked_statically() {
        let element = element("div", &[(":tabindex", "state.order")]);
        assert!(validate_a11y_attrs(&element).is_ok());
    }

    #[test]
    fn cursor_accepts_known_values() {
        for value in CURSOR_VALUES {
            let element = element("div", &[("cursor", value)]);
            assert!(
                validate_a11y_attrs(&element).is_ok(),
                "значение cursor=\"{value}\" должно приниматься"
            );
        }
    }

    #[test]
    fn cursor_rejects_unknown_value() {
        let element = element("div", &[("cursor", "hand")]);
        let error = validate_a11y_attrs(&element).expect_err("неизвестное значение cursor");
        assert!(error.to_string().contains("cursor"), "{}", error);
    }

    #[test]
    fn dynamic_cursor_is_not_checked_statically() {
        let element = element("div", &[(":cursor", "self.cursor")]);
        assert!(validate_a11y_attrs(&element).is_ok());
    }
}
