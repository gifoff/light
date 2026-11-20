use super::window::CURSOR_VALUES;
// Р РµР°Р»РёР·Р°С†РёСЏ Р°С‚СЂРёР±СѓС‚Р° `#[template]`: С‡С‚РµРЅРёРµ HTML-С„Р°Р№Р»Р° Рё СЂР°Р·СЂРµС€РµРЅРёРµ
// `<include>` РЅР° СЌС‚Р°РїРµ Р С”Р С•Р СР С—Р С‘Р В»РЎРЏРЎвЂ Р С‘Р С‘, РіРµРЅРµСЂР°С†РёСЏ СЂРµР°Р»РёР·Р°С†РёРё `Component`.

use proc_macro2::Span;

use light_fw_html::{Attr, Element};

pub(super) fn validate_a11y_attrs(element: &Element) -> syn::Result<()> {
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

pub(super) fn validate_attrs(attrs: &[Attr], known: &[&str]) -> syn::Result<()> {
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

pub(super) fn attr_value<'a>(attrs: &'a [Attr], name: &str) -> Option<&'a str> {
    attrs
        .iter()
        .find(|attr| attr.name == name)
        .map(|attr| attr.value.as_str())
}

/// Кодирует идентификатор окна в hex — для имени символа проверки уникальности.
pub(super) fn hex_id(id: &str) -> String {
    let mut out = String::with_capacity(id.len() * 2);
    for byte in id.bytes() {
        out.push_str(&format!("{byte:02x}"));
    }
    out
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
