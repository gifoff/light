//! Р РµР°Р»РёР·Р°С†РёСЏ Р°С‚СЂРёР±СѓС‚Р° `#[template]`: С‡С‚РµРЅРёРµ HTML-С„Р°Р№Р»Р° Рё СЂР°Р·СЂРµС€РµРЅРёРµ
//! `<include>` РЅР° СЌС‚Р°РїРµ Р С”Р С•Р СР С—Р С‘Р В»РЎРЏРЎвЂ Р С‘Р С‘, РіРµРЅРµСЂР°С†РёСЏ СЂРµР°Р»РёР·Р°С†РёРё `Component`.

use proc_macro2::{Span, TokenStream};
use quote::quote;

use super::attrs::{dynamic_value, style_token};
use super::keys::KeyExpr;
use super::parser::Generator;
use super::tokens::{setter_expr, text_segments};
use super::validation::{attr_value, validate_attrs};
use light_fw_html::{Element, Node};

impl Generator {
    pub(super) fn gen_radio(
        &mut self,
        element: &Element,
        key: &KeyExpr,
    ) -> syn::Result<TokenStream> {
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
        self.vmodel_bindings
            .push((model_root, "string".to_string(), radio_value));

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
    pub(super) fn gen_select(
        &mut self,
        element: &Element,
        key: &KeyExpr,
    ) -> syn::Result<TokenStream> {
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
        self.vmodel_bindings
            .push((model_root, "string".to_string(), None));

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
