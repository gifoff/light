//! Р РµР°Р»РёР·Р°С†РёСЏ Р°С‚СЂРёР±СѓС‚Р° `#[template]`: С‡С‚РµРЅРёРµ HTML-С„Р°Р№Р»Р° Рё СЂР°Р·СЂРµС€РµРЅРёРµ
//! `<include>` РЅР° СЌС‚Р°РїРµ Р С”Р С•Р СР С—Р С‘Р В»РЎРЏРЎвЂ Р С‘Р С‘, РіРµРЅРµСЂР°С†РёСЏ СЂРµР°Р»РёР·Р°С†РёРё `Component`.

use proc_macro2::{Span, TokenStream};
use quote::quote;

use super::attrs::{dynamic_value, style_token};
use super::keys::KeyExpr;
use super::parser::Generator;
use super::tokens::{setter_expr, text_segments};
use super::validation::{attr_value, validate_attrs};
use super::window::{asset_path, parse_bool, path_literal, png_dimensions};
use light_fw_html::Element;

impl Generator {
    pub(super) fn node_meta(&self, element: &Element) -> syn::Result<TokenStream> {
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
    pub(super) fn gen_container(
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
    pub(super) fn gen_titlebar(
        &mut self,
        element: &Element,
        key: &KeyExpr,
    ) -> syn::Result<TokenStream> {
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
    pub(super) fn gen_image(
        &mut self,
        element: &Element,
        key: &KeyExpr,
    ) -> syn::Result<TokenStream> {
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

    pub(super) fn gen_button(
        &mut self,
        element: &Element,
        key: &KeyExpr,
    ) -> syn::Result<TokenStream> {
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

    pub(super) fn gen_text_field(
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
    pub(super) fn disabled_token(&self, element: &Element) -> syn::Result<TokenStream> {
        Ok(match attr_value(&element.attrs, ":disabled") {
            Some(disabled) => self.path_expr(disabled)?,
            None => quote!(false),
        })
    }

    /// Строит флажок (`<checkbox>`) с `v-model` булева свойства.
    pub(super) fn gen_checkbox(
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
        self.vmodel_bindings
            .push((model_root, "bool".to_string(), None));

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
}
