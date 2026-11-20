//! Pipeline: reads the template HTML file, resolves `<include>` directives,
//! and generates a Rust `Component` implementation from the parsed tree.

//! Р РµР°Р»РёР·Р°С†РёСЏ Р°С‚СЂРёР±СѓС‚Р° `#[template]`: С‡С‚РµРЅРёРµ HTML-С„Р°Р№Р»Р° Рё СЂР°Р·СЂРµС€РµРЅРёРµ
//! `<include>` РЅР° СЌС‚Р°РїРµ Р С”Р С•Р СР С—Р С‘Р В»РЎРЏРЎвЂ Р С‘Р С‘, РіРµРЅРµСЂР°С†РёСЏ СЂРµР°Р»РёР·Р°С†РёРё `Component`.

use std::path::PathBuf;

use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{Ident, ItemStruct};

use super::methods::{BuiltMethods, build_methods};
use super::parser::Generator;
use super::parser::{load_nodes, normalize_src};
use super::state::BuiltState;
use super::state::build_state_methods;
use super::types::TemplateArgs;
use super::window::{parse_window, separate_window, window_tokens};
use light_fw_html::Node;

pub fn expand(attr: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
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
    let template_path = args.path.clone();
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
    let BuiltState {
        inspect: inspect_state_method,
        inject: inject_state_method,
    } = build_state_methods(&field_fields);

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    let full_path = manifest_dir.join(&relative);
    let mut stack = Vec::new();
    let loaded = load_nodes(&full_path, &mut stack)?;

    // Кастомные теги текущего пакета: `[package.metadata.light.provides.elements]`.
    // Читается *умершим* манифестом (см. `crate::provides`): если `Cargo.toml`
    // отсутствует или бит — пустой набор, стандартные элементы продолжают
    // работать, неизвестный тег даст понятную `compile_error!` (ТЗ 0.19.0 Этап 4).
    let custom_tags = crate::provides::read_provides_elements(&manifest_dir);

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

    // Инициализируем `custom_tags` при создании, чтобы не ловить
    // `clippy::field_reassign_with_default` (генератор остаётся `mut` —
    // в процессе генерации он накапливает `arms`, `scopes`, обработчики и т. д.).
    let mut generator = Generator {
        custom_tags,
        ..Default::default()
    };
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
    let BuiltMethods {
        invoke_handler: invoke_handler_method,
        dispatch_vmodel: dispatch_vmodel_method,
        eval_field: eval_field_method,
        validate_field: validate_field_method,
        validate_form: validate_form_method,
        modal: modal_method,
        windows: windows_method,
        id_symbol,
        track_files,
    } = build_methods(
        &args,
        &ident,
        &window,
        content,
        &field_fields,
        &arms,
        &click_handlers,
        &vmodel_bindings,
        &field_validators,
        &form_validators,
        &loaded.files,
    );

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
