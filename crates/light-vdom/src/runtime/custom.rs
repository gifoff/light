use std::cell::RefCell;
use std::collections::HashMap;

use crate::{
    component::Component,
    plugins,
    vdom::{NodeKind, Size, VElement, intern_key},
};

use super::fields::{child_key, render_segments};
use super::parse::build_element;
use super::style::attr_value;

/// Внутренний: реестр экземпляров кастомных элементов.
pub(crate) type CustomInstancesMap = HashMap<String, Box<dyn plugins::CustomElement>>;

thread_local! {
    /// Карта «ключ узла → экземпляр кастомного элемента».
    pub(crate) static INSTANCES: RefCell<CustomInstancesMap> = RefCell::new(HashMap::new());
}

/// Внутренний: регистрирует экземпляр кастомного элемента в thread-local реестре.
pub fn store_custom_instance(key: &str, instance: Box<dyn plugins::CustomElement>) {
    INSTANCES.with_borrow_mut(|map| {
        map.insert(key.to_owned(), instance);
    });
}

/// Выносит все накопленные экземпляры кастомных элементов (очищая карту).
///
/// Вызывается из [`light_fw_core::window::WindowState::redraw`] один раз перед
/// рендером компонента (чтобы сбросить записи предыдущего кадра), а затем
/// повторно после `component.render()` — чтобы забрать свежие экземпляры,
/// созданные в этом кадре. Возвращает пустую map, если карта пуста.
///
/// Публичный — чтобы интеграционные тесты могли вынести задрейненные
/// экземпляры и проверить контракт `on_event`.
pub fn drain_custom_instances() -> HashMap<String, Box<dyn plugins::CustomElement>> {
    INSTANCES.with_borrow_mut(std::mem::take)
}

/// Строит список дочерних узлов.
pub(crate) fn build_children<C: Component + ?Sized>(
    nodes: &[light_fw_html::Node],
    base: &str,
    parent_kind: &NodeKind,
    component: &C,
) -> Option<Vec<VElement>> {
    let mut out = Vec::with_capacity(nodes.len());
    let mut index = 0usize;
    for node in nodes {
        match node {
            light_fw_html::Node::Text(segments) => {
                let content = render_segments(segments, component)?;
                if content.is_empty() {
                    continue;
                }
                let key = child_key(base, index);
                let mut text = VElement::leaf(
                    intern_key(&key),
                    NodeKind::Text {
                        content: content.clone(),
                    },
                );
                // Растянуть Text по ширине родителя ТОЛЬКО в Column:
                // в Button/Row/TitleBar/Window — Auto (иначе circular layout).
                if matches!(parent_kind, NodeKind::Column) && matches!(text.style.width, Size::Auto)
                {
                    let mut s = text.style;
                    s.width = Size::Percent(100.0);
                    text = text.with_style(s);
                }
                out.push(text);
            }
            light_fw_html::Node::Element(element) => {
                let positional = child_key(base, index);
                // Ключ узла для диспетчеризации: явный `key` (литерал) имеет
                // приоритет; если его нет, берётся нормализованное имя
                // обработчика `@click` (без скобок) — так статический код макроса
                // может дотянуться до него через [`Component::invoke_handler`] и
                // [`Component::dispatch_by_name`] (встроенные имена `close`/`minimize`
                // и т. п. резолвятся в базовом хвосте трейта). Иначе — позиционный.
                let explicit_key = attr_value(&element.attrs, "key")
                    .map(str::trim)
                    .filter(|k| !k.is_empty() && !k.starts_with(':'));
                let click_name = attr_value(&element.attrs, "@click").map(normalize_click);
                let key = explicit_key
                    .or(click_name.as_deref().filter(|n| !n.is_empty()))
                    .unwrap_or(&positional);
                match build_element(element, key, None, component) {
                    Some(child) => out.push(child),
                    // `v-if="..."` с ложным условием — узел «скрываем»
                    // легитимно: пропускаем только его, не прерывая остаток
                    // списка и не откатывая всё дерево в fallback. Настоящая
                    // ошибка разбора (узел без v-if) по-прежнему откатывает
                    // runtime-рендер на статическое дерево (return None).
                    None if attr_value(&element.attrs, "v-if").is_some() => {}
                    None => return None,
                };
            }
        }
        index += 1;
    }
    Some(out)
}

/// Имя обработчика `@click` для диспетчеризации по имени.
///
/// Убирает скобки вызова (`"increment()"` → `"increment"`), пробельные
/// символы вокруг и ведущее `this.` — генерируемый макросом
/// [`Component::dispatch_by_name`] сопоставляет именно это имя.
fn normalize_click(handler: &str) -> String {
    let trimmed = handler.trim();
    let trimmed = trimmed.strip_prefix("this.").unwrap_or(trimmed);
    trimmed
        .strip_suffix("()")
        .map(str::trim)
        .unwrap_or(trimmed)
        .to_owned()
}
