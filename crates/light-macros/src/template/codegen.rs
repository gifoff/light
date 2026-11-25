//! Р РµР°Р»РёР·Р°С†РёСЏ Р°С‚СЂРёР±СѓС‚Р° `#[template]`: С‡С‚РµРЅРёРµ HTML-С„Р°Р№Р»Р° Рё СЂР°Р·СЂРµС€РµРЅРёРµ
//! `<include>` РЅР° СЌС‚Р°РїРµ Р С”Р С•Р СР С—Р С‘Р В»РЎРЏРЎвЂ Р С‘Р С‘, РіРµРЅРµСЂР°С†РёСЏ СЂРµР°Р»РёР·Р°С†РёРё `Component`.

use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::Ident;

use super::attrs::escape_format;
use super::keys::KeyExpr;
use super::keys::{child_key, condition_token, element_directive, has_attr, parse_for, static_key};
use super::parser::Generator;
use super::tokens::{
    is_plain_click_handler, is_plain_mouse_handler, normalize_click_name, normalize_mouse_name,
    split_path, to_ident,
};
use super::validation::{attr_value, validate_a11y_attrs, validate_attrs};
use super::window::parse_bool;
use light_fw_html::{Element, Node, Segment};

impl Generator {
    /// Выражение пути с учётом локальных переменных `v-for`.
    ///
    /// Если первый сегмент совпадает с именем переменной цикла, путь строится
    /// от неё (`item`, `item.name()`), иначе — от `self` (`self.items()`).
    pub(super) fn path_expr(&self, path: &str) -> syn::Result<TokenStream> {
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
    pub(super) fn visible_token(&self, element: &Element) -> syn::Result<TokenStream> {
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
    pub(super) fn z_token(&self, element: &Element) -> syn::Result<TokenStream> {
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
    pub(super) fn record_field_validator(
        &mut self,
        element: &Element,
        key: &KeyExpr,
    ) -> syn::Result<()> {
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
    pub(super) fn record_form_validator(
        &mut self,
        element: &Element,
        key: &KeyExpr,
    ) -> syn::Result<()> {
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
    pub(super) fn content_expr(&self, segments: &[Segment]) -> syn::Result<TokenStream> {
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
    pub(super) fn gen_children(
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
    pub(super) fn gen_chain(
        &mut self,
        branches: &[(Option<&str>, &Node, KeyExpr)],
    ) -> syn::Result<TokenStream> {
        let mut chain = TokenStream::new();
        for (position, (condition, node, key)) in branches.iter().enumerate() {
            let body = self.gen_push_node(node, key)?;
            match condition {
                Some(condition) => {
                    let condition = condition_token(self, condition)?;
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
    pub(super) fn gen_push_node(&mut self, node: &Node, key: &KeyExpr) -> syn::Result<TokenStream> {
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
    pub(super) fn gen_for_push(
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
    pub(super) fn disabled_element_feature(name: &str) -> Option<&'static str> {
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
    pub(super) fn gen_element(
        &mut self,
        element: &Element,
        key: &KeyExpr,
    ) -> syn::Result<TokenStream> {
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
        // Кастомный тег (ТЗ 0.19.0 Этап 4, Plugin API): объявлен в
        // `[package.metadata.light.provides.elements]` текущего пакета →
        // рендерится как `NodeKind::Custom` через плагинную фабрику
        // (`runtime::build_custom_element`), атрибуты и дети передаются ядром.
        // Прочие нестандартные теги — ошибка компиляции с подсказкой, как
        // объявить свой элемент.
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
            name if self.custom_tags.contains(name) => self.gen_custom_element(element, key),
            other => Err(syn::Error::new(
                Span::call_site(),
                format!(
                    "неизвестный элемент <{other}>; поддерживаются: \
                     <div>, <span>, <h1>…<h5>, <text>, <password>, <checkbox>, <radio>, \
                     <select>, <option>, <button>, <titlebar>, <image>, <include>, <window>. \
                     Для собственного элемента объявите тег в [package.metadata.light.provides] \
                     elements в Cargo.toml и реализуйте его через #[component] (Plugin API)"
                ),
            )),
        }
    }

    /// Кастомный элемент (плагин, ТЗ 0.19.0 Этап 4): объявлен в
    /// `[package.metadata.light.provides.elements]` текущего пакета.
    ///
    /// Статически генерируется как контейнер `NodeKind::Custom(<тег>)`.
    /// На самом деле контент подставляется runtime'ом в
    /// `runtime::build_custom_element`, где фабрика (созданная через
    /// `#[linkme::distributed_slice(REGISTERED_COMPONENTS)]` в `#[component]`)
    /// вызывает `render` плагина и перекрашивает корень в `Custom(тег)`.
    /// Атрибуты и дочерние узлы из HTML передаются плагину тем же путём.
    ///
    /// Важно: генерирует **блок-выражение** (как `gen_container`), а не
    /// statement. Вызывающий код (`gen_push_node`) оборачивает результат в
    /// `__children.push(#element);` — поэтому здесь свой `let mut __children`
    /// создаётся внутри блока, а на выходе возвращается `VElement` как
    /// значение последнего выражения.
    pub(super) fn gen_custom_element(
        &mut self,
        element: &Element,
        key: &KeyExpr,
    ) -> syn::Result<TokenStream> {
        let tag_literal = element.name.clone();
        let pushes = self.gen_children(&element.children, Some(key))?;
        let key = key.token();
        Ok(quote! {{
            let mut __children = ::std::vec::Vec::new();
            #(#pushes)*
            ::light_fw::vdom::VElement::container(
                #key,
                ::light_fw::vdom::NodeKind::Custom(::light_fw::vdom::intern_key(#tag_literal)),
                __children,
            )
        }})
    }

    /// Ветви обработчиков событий элемента; регистрируются в `arms`.
    pub(super) fn push_events(&mut self, element: &Element, key: &TokenStream) -> syn::Result<()> {
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
        // `@mouseenter`/`@mouseleave`: отдельный список (`mouse_handlers`) —
        // мышиный обработчик без единого `@click` в шаблоне тоже обязан
        // получать ветку `invoke_handler` (0.21.0). Имя нормализуется так же,
        // как в `light-vdom::runtime` (атрибуты `data-event-enter`/`data-event-leave`),
        // поэтому сопоставление с runtime-деревом точное. Дедупликация по имени:
        // несколько узлов с одним именем — одна рабочая ветка.
        for attr in ["@mouseenter", "@mouseleave"] {
            if let Some(handler) = attr_value(&element.attrs, attr)
                && is_plain_mouse_handler(handler)
            {
                let call = self.handler_call(handler, None)?;
                let name = normalize_mouse_name(handler);
                if !self.mouse_handlers.iter().any(|(n, _)| n == &name) {
                    self.mouse_handlers.push((name, call));
                }
            }
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
    pub(super) fn event_branches(&self, element: &Element) -> syn::Result<Vec<TokenStream>> {
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
}
