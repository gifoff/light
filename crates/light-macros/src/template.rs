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
            ) -> ::std::option::Option<::std::boxed::Box<dyn ::light::component::Component>> {
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
        None => quote!(::light::component::WindowConfig::default()),
    };
    let render_body = if window.is_some() || !nodes.is_empty() {
        quote! {
            ::light::vdom::VElement::container(
                "",
                ::light::vdom::NodeKind::Window,
                __children,
            )
            .with_window(#window_config)
        }
    } else {
        quote! {
            ::light::vdom::VElement::container(
                "",
                ::light::vdom::NodeKind::Column,
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
        ..
    } = generator;

    // Пользовательские проверки полей (`:validator`) и форм (`@validate`).
    let field_validator_arms = field_validators
        .iter()
        .map(|(key, call)| quote!(#key => #call,));
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
        quote! {
            #key => ::std::option::Option::Some(match #call {
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
            fn modal(&self) -> ::std::option::Option<::std::boxed::Box<dyn ::light::Component>> {
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
                (|| -> ::std::boxed::Box<dyn ::light::Component> {
                    ::std::boxed::Box::new(#path::new())
                }) as fn() -> ::std::boxed::Box<dyn ::light::Component>
            }
        });
        quote! {
            fn windows(
                &self,
            ) -> ::std::vec::Vec<fn() -> ::std::boxed::Box<dyn ::light::Component>> {
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

        #(#track_files)*
        #id_symbol

        impl ::light::Component for #ident {
            fn render(&self) -> ::light::vdom::VElement {
                let mut __children = ::std::vec::Vec::new();
                #(#pushes)*
                #render_body
            }

            fn dispatch(&self, __key: &str, __event: ::light::vdom::Event) -> bool {
                #[allow(unused_imports)]
                use ::light::dom::Dom;
                #[allow(unused_imports)]
                use ::light::vdom::{VNode as _, VWindow as _};
                #(#arms)*
                false
            }

            #modal_method
            #window_factory_method
            #windows_method
            #validate_field_method
            #validate_form_method
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
}

/// Ключ узла: известный на этапе компиляции либо вычисляемый во время работы
/// (`v-for`). Статические ключи остаются `&'static str`-литералами, динамические
/// интернируются через [`light::vdom::intern_key`].
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
                Self::Dyn(quote!(::light::vdom::intern_key(
                    &::std::format!("{}{}", #expr, #suffix)
                )))
            }
        }
    }

    /// Ключ элемента в цикле `v-for` (суффикс — индекс итерации).
    fn iteration(&self) -> Self {
        match self {
            Self::Lit(base) => Self::Dyn(quote!(::light::vdom::intern_key(
                &::std::format!("{}/{}", #base, __for_index)
            ))),
            Self::Dyn(expr) => Self::Dyn(quote!(::light::vdom::intern_key(
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
                    __children.push(::light::vdom::VElement::leaf(
                        #key,
                        ::light::vdom::NodeKind::Text { content: #content },
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
                let element = self.gen_element(element, key)?;
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
            _ => return None,
        };
        let enabled = [
            ("elements-input", cfg!(feature = "elements-input")),
            ("elements-checkbox", cfg!(feature = "elements-checkbox")),
            ("elements-radio", cfg!(feature = "elements-radio")),
            ("elements-select", cfg!(feature = "elements-select")),
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
                    "элемент <{}> недоступен: Cargo-фича `{}` выключена у крейта `light`\n\
                     help: включите её в зависимости, например \
                     `light = {{ features = [\"{}\"] }}`",
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
            other => Err(syn::Error::new(
                Span::call_site(),
                format!(
                    "неизвестный элемент <{other}>; поддерживаются: \
                     <div>, <span>, <h1>…<h5>, <text>, <password>, <checkbox>, <radio>, \
                     <select>, <option>, <button>, <titlebar>, <include>, <template>"
                ),
            )),
        }
    }

    /// Ветви обработчиков событий элемента; регистрируются в `arms`.
    fn push_events(&mut self, element: &Element, key: &TokenStream) -> syn::Result<()> {
        let branches = self.event_branches(element)?;
        if !branches.is_empty() {
            self.arms.push(quote! {
                if __key == #key { #(#branches)* }
            });
        }
        Ok(())
    }

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
                branches.push(quote! {
                    if ::std::matches!(&__event, ::light::vdom::Event::#variant) {
                        #call; return true;
                    }
                });
            }
        }
        for (attr_name, variant) in [("@mousedown", "Mousedown"), ("@mouseup", "Mouseup")] {
            if let Some(handler) = attr_value(&element.attrs, attr_name) {
                let call = self.handler_call(handler, Some(quote!(*__button)))?;
                let variant = Ident::new(variant, Span::call_site());
                branches.push(quote! {
                    if let ::light::vdom::Event::#variant(__button) = &__event {
                        #call; return true;
                    }
                });
            }
        }
        if let Some(handler) = attr_value(&element.attrs, "@mousemove") {
            let call = self.handler_call(handler, Some(quote!(*__x, *__y)))?;
            branches.push(quote! {
                if let ::light::vdom::Event::Mousemove { x: __x, y: __y } = &__event {
                    #call; return true;
                }
            });
        }
        for (attr_name, variant) in [("@keydown", "Keydown"), ("@keyup", "Keyup")] {
            if let Some(handler) = attr_value(&element.attrs, attr_name) {
                let call = self.handler_call(handler, Some(quote!(__info.clone())))?;
                let variant = Ident::new(variant, Span::call_site());
                branches.push(quote! {
                    if let ::light::vdom::Event::#variant(__info) = &__event {
                        #call; return true;
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
            if args.is_some() && position == last {
                expr = quote!(#expr.#ident);
            } else {
                expr = quote!(#expr.#ident());
            }
        }
        Ok(match args {
            Some(args) => quote!(#expr(#args)),
            None => expr,
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
                || name == "required-indicator"
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
            quote!(::light::vdom::NodeKind::Row)
        } else {
            quote!(::light::vdom::NodeKind::Column)
        };
        let pushes = self.gen_children(&element.children, Some(key))?;
        let style = style_token(self, element, default_font, self.visible_token(element)?)?;
        let key = key.token();
        self.push_events(element, &key)?;
        Ok(quote! {{
            let mut __children = ::std::vec::Vec::new();
            #(#pushes)*
            ::light::vdom::VElement::container(#key, #kind, __children)
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
            ::light::vdom::VElement::container(#key, ::light::vdom::NodeKind::TitleBar, __children)
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
            Some(value) => quote!(::light::vdom::parse_button_type(&#value)),
            None => match attr_value(&element.attrs, "type").map(str::trim) {
                None | Some("") | Some("submit") => quote!(::light::vdom::ButtonType::Submit),
                Some("button") => quote!(::light::vdom::ButtonType::Button),
                Some("reset") => quote!(::light::vdom::ButtonType::Reset),
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
            ::light::vdom::VElement::container(
                #key,
                ::light::vdom::NodeKind::Button {
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
                Some(value) => quote!(::light::vdom::parse_bool(&#value)),
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
            quote!(::light::vdom::Resize::None)
        } else {
            match dynamic_value(self, element, "resize")? {
                Some(value) => quote!(::light::vdom::parse_resize(&#value)),
                None => match attr_value(&element.attrs, "resize").map(str::trim) {
                    None | Some("none") => quote!(::light::vdom::Resize::None),
                    Some("horizontal") => quote!(::light::vdom::Resize::Horizontal),
                    Some("vertical") => quote!(::light::vdom::Resize::Vertical),
                    Some("both") => quote!(::light::vdom::Resize::Both),
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
            if let ::light::vdom::Event::TextChange(__value) = &__event {
                #write; return true;
            }
        });
        self.arms.push(quote! {
            if __key == #key { #(#branches)* }
        });

        let style = style_token(self, element, None, self.visible_token(element)?)?;
        let meta = self.node_meta(element)?;
        Ok(quote! {
            ::light::vdom::VElement::leaf(
                #key,
                ::light::vdom::NodeKind::TextField {
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
            if let ::light::vdom::Event::Click = &__event {
                #write; return true;
            }
        });
        self.arms.push(quote! {
            if __key == #key { #(#branches)* }
        });

        let style = style_token(self, element, None, self.visible_token(element)?)?;
        let meta = self.node_meta(element)?;
        Ok(quote! {
            ::light::vdom::VElement::leaf(
                #key,
                ::light::vdom::NodeKind::Checkbox {
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
            if let ::light::vdom::Event::Click = &__event {
                #write; return true;
            }
        });
        self.arms.push(quote! {
            if __key == #key { #(#branches)* }
        });

        let style = style_token(self, element, None, self.visible_token(element)?)?;
        let meta = self.node_meta(element)?;
        Ok(quote! {
            ::light::vdom::VElement::leaf(
                #key,
                ::light::vdom::NodeKind::Radio {
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
                ::light::vdom::SelectOption {
                    label: (#label).clone(),
                    value: #value,
                }
            });
        }
        let options = quote!(::std::vec![#(#options),*]);

        let key = key.token();
        let mut branches = self.event_branches(element)?;
        branches.push(quote! {
            if let ::light::vdom::Event::TextChange(__value) = &__event {
                #write; return true;
            }
        });
        self.arms.push(quote! {
            if __key == #key { #(#branches)* }
        });

        let style = style_token(self, element, None, self.visible_token(element)?)?;
        let meta = self.node_meta(element)?;
        Ok(quote! {
            ::light::vdom::VElement::leaf(
                #key,
                ::light::vdom::NodeKind::Select {
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
        quote!(::light::vdom::Size::Percent(#number))
    } else {
        let number = value.strip_suffix("px").unwrap_or(value).trim();
        let number: f32 = number.parse().map_err(|_| invalid())?;
        quote!(::light::vdom::Size::Px(#number))
    };
    Ok(Some(token))
}

/// Собирает [`light::vdom::Style`] из атрибутов: `width`, `height` (px или %),
/// `position-x` (`left`/`center`/`right`), `position-y` (`top`/`center`/
/// `bottom`), `position-offset-x`/`position-offset-y` (px или %, в том числе
/// отрицательные; псевдонимы `left`/`top`), `padding`/`margin` и их
/// построчные варианты `padding-top`/`padding-right`/`padding-bottom`/
/// `padding-left` (аналогично `margin-*`), `font-size`,
/// `border-size` (px), `background`, `font-color`, `border-color` (цвет),
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
        quote!(::light::vdom::Size::Auto),
    )?;
    let height = size_attr(
        generator,
        element,
        "height",
        quote!(::light::vdom::Size::Auto),
    )?;
    let min_width = optional_size_attr(generator, element, "min-width")?;
    let min_height = optional_size_attr(generator, element, "min-height")?;
    let max_width = optional_size_attr(generator, element, "max-width")?;
    let max_height = optional_size_attr(generator, element, "max-height")?;
    let align_x = align_attr(
        generator,
        element,
        "position-x",
        "left",
        "right",
        quote!(::light::vdom::parse_align_x),
        quote!(::light::vdom::Align::Start),
    )?;
    let align_y = align_attr(
        generator,
        element,
        "position-y",
        "top",
        "bottom",
        quote!(::light::vdom::parse_align_y),
        quote!(::light::vdom::Align::Start),
    )?;
    let offset_x = offset_attr(generator, element, "position-offset-x")?;
    let offset_y = offset_attr(generator, element, "position-offset-y")?;
    let justify_content = align_attr(
        generator,
        element,
        "justify-content",
        "start",
        "end",
        quote!(::light::vdom::parse_justify),
        quote!(::light::vdom::Align::Start),
    )?;
    let align_items = align_attr(
        generator,
        element,
        "align-items",
        "start",
        "end",
        quote!(::light::vdom::parse_align_items),
        quote!(::light::vdom::Align::Start),
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
    Ok(quote!(::light::vdom::Style {
        width: #width,
        height: #height,
        min_width: #min_width,
        min_height: #min_height,
        max_width: #max_width,
        max_height: #max_height,
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
        value if value == start => quote!(::light::vdom::Align::Start),
        "center" => quote!(::light::vdom::Align::Center),
        value if value == end => quote!(::light::vdom::Align::End),
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
        return Ok(quote!(::light::vdom::Offset::Px(0.0)));
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
        quote!(::light::vdom::Offset::Percent(#number))
    } else {
        let number = value.strip_suffix("px").unwrap_or(value).trim();
        let number: f32 = number.parse().map_err(|_| invalid())?;
        quote!(::light::vdom::Offset::Px(#number))
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
        return Ok(quote!(::light::vdom::parse_size_or_auto(&#value)));
    }
    Ok(parse_size_attr(element, name)?.unwrap_or(default))
}

/// Необязательный размер (`min-width`/`max-width` и т. п.) как
/// `Option<::light::vdom::Size>` с учётом динамической привязки.
fn optional_size_attr(
    generator: &Generator,
    element: &Element,
    name: &str,
) -> syn::Result<TokenStream> {
    if let Some(value) = dynamic_value(generator, element, name)? {
        return Ok(quote!(::std::option::Option::Some(
            ::light::vdom::parse_size_or_auto(&#value)
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
        return Ok(quote!(::light::vdom::parse_offset(&#value)));
    }
    offset_token(element, name)
}

/// Отступы (`padding`/`margin`): динамический `:name="путь"` задаёт базовое
/// значение для всех сторон, статические `name-*` переопределяют отдельные.
fn edges_attr(generator: &Generator, element: &Element, base: &str) -> syn::Result<TokenStream> {
    let Some(value) = dynamic_value(generator, element, base)? else {
        return edges_token(element, base);
    };
    let scalar = quote!(::light::vdom::parse_px(&#value));
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
    Ok(quote!(::light::vdom::Edges {
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
        return Ok(Some(quote!(::light::vdom::parse_px(&#value))));
    }
    Ok(parse_px_attr(element, name)?.map(|value| quote!(#value)))
}

/// Цветовой атрибут (`background`/`font-color`/`border-color`) как `Option<u32>`
/// с учётом динамической привязки.
fn color_attr(generator: &Generator, element: &Element, name: &str) -> syn::Result<TokenStream> {
    if let Some(value) = dynamic_value(generator, element, name)? {
        return Ok(quote!(::light::vdom::parse_color(&#value)));
    }
    Ok(color_token(parse_color_attr(element, name)?))
}

/// Выражение флага `required` с учётом динамической привязки `:required`.
fn required_attr(generator: &Generator, element: &Element) -> syn::Result<TokenStream> {
    if let Some(value) = dynamic_value(generator, element, "required")? {
        return Ok(quote!(::light::vdom::parse_bool(&#value)));
    }
    Ok(required_token(element))
}

/// Выражение индикатора обязательности с учётом динамической привязки
/// `:required-indicator`.
fn required_indicator_attr(generator: &Generator, element: &Element) -> syn::Result<TokenStream> {
    if let Some(value) = dynamic_value(generator, element, "required-indicator")? {
        return Ok(quote!(::light::vdom::parse_required_indicator(&#value)));
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

/// Собирает [`light::vdom::Edges`] из базового атрибута (`padding`/`margin`) и
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
    Ok(quote!(::light::vdom::Edges {
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
        return Ok(quote!(::light::vdom::RequiredIndicator::Asterisk));
    };
    let value = value.trim();
    let token = match value {
        "" | "*" => quote!(::light::vdom::RequiredIndicator::Asterisk),
        "none" => quote!(::light::vdom::RequiredIndicator::None),
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
            quote!(::light::vdom::RequiredIndicator::Accent(#color))
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
/// [`light::vdom::Cursor::from_name`]).
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
    })
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
    let mut builder = quote! { let mut cfg = ::light::component::WindowConfig::default(); };
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
