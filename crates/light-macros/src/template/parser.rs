//! Р РµР°Р»РёР·Р°С†РёСЏ Р°С‚СЂРёР±СѓС‚Р° `#[template]`: С‡С‚РµРЅРёРµ HTML-С„Р°Р№Р»Р° Рё СЂР°Р·СЂРµС€РµРЅРёРµ
//! `<include>` РЅР° СЌС‚Р°РїРµ Р С”Р С•Р СР С—Р С‘Р В»РЎРЏРЎвЂ Р С‘Р С‘, РіРµРЅРµСЂР°С†РёСЏ СЂРµР°Р»РёР·Р°С†РёРё `Component`.

use std::path::{Path, PathBuf};

use proc_macro2::{Span, TokenStream};

use super::types::Loaded;
use light_fw_html::{Attr, Node};

pub(super) fn normalize_src(value: &str) -> Result<String, String> {
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
pub(super) fn load_nodes(path: &Path, stack: &mut Vec<PathBuf>) -> syn::Result<Loaded> {
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
pub(super) fn resolve_node(
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

pub(super) fn validate_include(attrs: &[Attr]) -> syn::Result<()> {
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
pub(super) struct Generator {
    pub(super) arms: Vec<TokenStream>,
    /// Кастомные теги текущего пакета (из `[package.metadata.light.provides.elements]`).
    /// Разрешены в HTML-шаблонах и рендерятся как `NodeKind::Custom` через
    /// плагинную фабрику; прочие нестандартные теги — `compile_error!`
    /// (ТЗ 0.19.0 Этап 4, Plugin API).
    pub(super) custom_tags: std::collections::HashSet<String>,
    /// РРјРµРЅР° локальных переменных `v-for`, видимых в текущей вложенности.
    pub(super) scopes: Vec<String>,
    /// Пользовательские проверки полей: `(ключ узла, вызов метода)`.
    pub(super) field_validators: Vec<(String, TokenStream)>,
    /// Пользовательские проверки форм: `(ключ формы, вызов метода)`.
    pub(super) form_validators: Vec<(String, TokenStream)>,
    /// Обработчики `@click` для runtime-диспетчеризации: `(ключ узла как токен,
    /// вызов метода)`. Токен ключа — выражение типа `&'static str`.
    /// Ветви `@click` для runtime-диспетчеризации: (ключ узла из статического
    /// дерева, (вызов метода, имя обработчика без скобок)). Имя используется в
    /// `invoke_handler`, где runtime-дерево дотягивается до обработчика по имени.
    pub(super) click_handlers: Vec<(TokenStream, (TokenStream, String))>,
    /// Привязки v-model для runtime-writeback: (имя поля, тип поля, значение radio).
    /// Checkbox → (field, "bool", None); Radio → (field, "string", Some(value));
    /// Select → (field, "string", None). Используется в `dispatch_vmodel`,
    /// который обходит runtime-дерево по ключу и записывает новое значение.
    pub(super) vmodel_bindings: Vec<(String, String, Option<String>)>,
}
