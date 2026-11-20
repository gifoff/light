use crate::{
    component::Component,
    vdom::{NodeKind, Resize, SelectOption, VElement, intern_key},
};

use super::parse::parse_fail;
#[allow(unused_imports)]
use super::style::{
    attr_value, eval_bool, eval_simple, parse_resize_value, resolve_disabled, text_content,
};

/// Внутренний: рендер сегментов (literal + path).
pub(crate) fn render_segments<C: Component + ?Sized>(
    segments: &[light_fw_html::Segment],
    component: &C,
) -> Option<String> {
    let mut out = String::new();
    for segment in segments {
        match segment {
            light_fw_html::Segment::Literal(text) => out.push_str(text),
            light_fw_html::Segment::Path(path) => {
                let value = match eval_simple(component, path) {
                    Some(v) => v,
                    None => {
                        return None;
                    }
                };
                out.push_str(&value);
            }
        }
    }
    Some(out)
}

/// Детерминированный ключ узла: `base#index`. Ключи стабильны между перерендерами
/// одного шаблона (порядок узлов не меняется), поэтому сопоставление событий и
/// отслеживание реактивности работают без изменений дерева.
pub(crate) fn child_key(base: &str, index: usize) -> String {
    format!("{base}.{index}")
}

/// Внутренний: ключ корневого узла.
pub(crate) const ROOT_KEY: &str = "";

// ---------------------------------------------------------------------------
// Листы
// ---------------------------------------------------------------------------

/// `<text>` / `<password>`: значение из `v-model` (если простой идентификатор),
/// остальные свойства из атрибутов.
/// Внутренний: текстовое поле.
pub(crate) fn build_text_field<C: Component + ?Sized>(
    element: &light_fw_html::Element,
    key: &str,
    component: &C,
) -> Option<VElement> {
    let masked = element.name == "password";
    let model = attr_value(&element.attrs, "v-model");
    let value = match model {
        Some(m) => eval_simple(component, m)?,
        None => String::new(),
    };
    let placeholder = attr_value(&element.attrs, "placeholder")
        .unwrap_or("")
        .to_string();
    let disabled = resolve_disabled(&element.attrs, component)?;
    let multiline = attr_value(&element.attrs, "textarea").is_some();
    let resize = match attr_value(&element.attrs, "resize") {
        Some(r) => {
            let parsed = parse_resize_value(r);
            if parsed.is_none() {
                parse_fail(&element.name, "resize", r);
            }
            parsed?
        }
        None => Resize::default(),
    };
    Some(VElement::leaf(
        intern_key(key),
        NodeKind::TextField {
            value,
            placeholder,
            disabled,
            multiline,
            resize,
            masked,
        },
    ))
}

#[cfg(feature = "elements-checkbox")]
pub(crate) fn build_checkbox<C: Component + ?Sized>(
    element: &light_fw_html::Element,
    key: &str,
    component: &C,
) -> Option<VElement> {
    let label = text_content(&element.children)?;
    let model = attr_value(&element.attrs, "v-model")?;
    let checked = eval_bool(component, model)?;
    let disabled = resolve_disabled(&element.attrs, component)?;
    let leaf = VElement::leaf(
        intern_key(key),
        NodeKind::Checkbox {
            label,
            checked,
            disabled,
        },
    );
    // data-v-model добавляется в build_element через attrs (см. там).
    Some(leaf)
}

#[cfg(not(feature = "elements-checkbox"))]
pub(crate) fn build_checkbox<C: Component + ?Sized>(
    _element: &light_fw_html::Element,
    _key: &str,
    _component: &C,
) -> Option<VElement> {
    None
}

#[cfg(feature = "elements-radio")]
pub(crate) fn build_radio<C: Component + ?Sized>(
    element: &light_fw_html::Element,
    key: &str,
    component: &C,
) -> Option<VElement> {
    let label = text_content(&element.children)?;
    let value = attr_value(&element.attrs, "value")
        .unwrap_or("")
        .to_string();
    let model = attr_value(&element.attrs, "v-model")?;
    let current = eval_simple(component, model)?;
    let selected = current == value;
    let disabled = resolve_disabled(&element.attrs, component)?;
    let leaf = VElement::leaf(
        intern_key(key),
        NodeKind::Radio {
            label,
            value,
            selected,
            disabled,
        },
    );
    // data-v-model и data-value добавляются в build_element через attrs.
    Some(leaf)
}

#[cfg(not(feature = "elements-radio"))]
pub(crate) fn build_radio<C: Component + ?Sized>(
    _element: &light_fw_html::Element,
    _key: &str,
    _component: &C,
) -> Option<VElement> {
    None
}

#[cfg(feature = "elements-select")]
pub(crate) fn build_select<C: Component + ?Sized>(
    element: &light_fw_html::Element,
    key: &str,
    component: &C,
) -> Option<VElement> {
    let model = attr_value(&element.attrs, "v-model")?;
    let value = eval_simple(component, model)?;
    let options = collect_options(&element.children)?;
    let disabled = resolve_disabled(&element.attrs, component)?;
    let leaf = VElement::leaf(
        intern_key(key),
        NodeKind::Select {
            value,
            options,
            disabled,
        },
    );
    // data-v-model добавляется в build_element через attrs.
    Some(leaf)
}

#[cfg(not(feature = "elements-select"))]
pub(crate) fn build_select<C: Component + ?Sized>(
    _element: &light_fw_html::Element,
    _key: &str,
    _component: &C,
) -> Option<VElement> {
    None
}

#[cfg(feature = "elements-select")]
fn collect_options(children: &[light_fw_html::Node]) -> Option<Vec<SelectOption>> {
    let mut options = Vec::new();
    for child in children {
        let light_fw_html::Node::Element(option) = child else {
            continue;
        };
        if option.name != "option" {
            continue;
        }
        let label = text_content(&option.children)?;
        let value = attr_value(&option.attrs, "value")
            .map(str::to_string)
            .unwrap_or(label.clone());
        options.push(SelectOption { label, value });
    }
    Some(options)
}

#[cfg(not(feature = "elements-select"))]
#[allow(dead_code)]
fn collect_options(_children: &[light_fw_html::Node]) -> Option<Vec<SelectOption>> {
    None
}
