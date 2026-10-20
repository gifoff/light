//! Глобальный доступ к виртуальному дереву приложения.
//!
//! Оконный бэкенд сохраняет отрисованное дерево каждого окна; объект [`Dom`]
//! даёт к ним доступ: получить дерево активного окна, найти узел по ключу,
//! перебрать все узлы как [`Collection`].

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use crate::collection::Collection;
use crate::control::{WindowToken, active_window};
use crate::vdom::VElement;

thread_local! {
    /// Деревья отрисованных окон: токен окна → виртуальное дерево.
    static TREES: RefCell<HashMap<WindowToken, Arc<VElement>>> = RefCell::new(HashMap::new());
}

/// Сохраняет дерево окна `token` (вызывается оконным бэкендом).
#[cfg(feature = "window")]
pub(crate) fn set_tree(token: WindowToken, tree: Arc<VElement>) {
    TREES.with(|trees| {
        trees.borrow_mut().insert(token, tree);
    });
}

/// Удаляет дерево закрытого окна.
#[cfg(feature = "window")]
pub(crate) fn remove_tree(token: WindowToken) {
    TREES.with(|trees| {
        trees.borrow_mut().remove(&token);
    });
}

/// Глобальный доступ к виртуальному дереву приложения.
///
/// Все методы работают с деревом **активного** окна — того, которое последним
/// обрабатывало событие (по умолчанию — первое созданное). Это позволяет из
/// любого обработчика обращаться к текущему интерфейсу без хранения ссылок.
pub struct Dom;

impl Dom {
    /// Дерево активного окна.
    pub fn tree() -> Option<Arc<VElement>> {
        let token = active_window();
        TREES.with(|trees| trees.borrow().get(&token).cloned())
    }

    /// Узел активного окна по ключу.
    pub fn get(key: &str) -> Option<VElement> {
        Self::tree()?.get(key).cloned()
    }

    /// Все узлы дерева активного окна (обход в глубину) как коллекция.
    pub fn nodes() -> Collection<VElement> {
        match Self::tree() {
            Some(tree) => tree.descendants().into_iter().cloned().collect(),
            None => Collection::new(),
        }
    }

    /// Число узлов в дереве активного окна.
    pub fn len() -> usize {
        Self::tree().map_or(0, |tree| tree.descendants().count())
    }

    /// Отрисовано ли дерево активного окна.
    pub fn is_empty() -> bool {
        Self::tree().is_none()
    }

    /// Узлы активного окна, удовлетворяющие предикату, как коллекция.
    ///
    /// Возвращается [`Collection`], поэтому по результату можно цепочкой
    /// вызывать `first`, `map`, `each` и т. п.
    pub fn find(mut predicate: impl FnMut(&VElement) -> bool) -> Collection<VElement> {
        match Self::tree() {
            Some(tree) => tree
                .descendants()
                .into_iter()
                .filter(|node| predicate(node))
                .cloned()
                .collect(),
            None => Collection::new(),
        }
    }

    /// Все узлы активного окна, удовлетворяющие предикату (синоним [`Dom::find`]).
    pub fn find_all(predicate: impl FnMut(&VElement) -> bool) -> Collection<VElement> {
        Self::find(predicate)
    }

    /// Корневой узел активного окна, если он является окном ([`NodeKind::Window`]).
    ///
    /// Через него выполняются операции над окном: `Dom::window().close()`.
    pub fn window() -> Option<VElement> {
        let tree = Self::tree()?;
        tree.window_config().map(|_| (*tree).clone())
    }

    /// Первый узел активного окна с указанным именем HTML-тега.
    pub fn find_by_tag(tag: &str) -> Option<VElement> {
        Self::tree()?.find_by_tag(tag).cloned()
    }

    /// Первый узел активного окна с атрибутом `name`, равным `value`.
    pub fn find_by_attr(name: &str, value: &str) -> Option<VElement> {
        Self::tree()?.find_by_attr(name, value).cloned()
    }

    /// Первый узел активного окна с заданным содержимым (текст или кнопка).
    pub fn find_by_content(content: &str) -> Option<VElement> {
        Self::tree()?.find_by_content(content).cloned()
    }

    /// Первый узел активного окна, содержимое которого содержит `needle`.
    pub fn find_containing(needle: &str) -> Option<VElement> {
        Self::tree()?.find_containing(needle).cloned()
    }
}

#[cfg(all(test, feature = "window"))]
mod tests {
    use super::*;
    use crate::vdom::NodeKind;

    #[test]
    fn stores_and_reads_active_tree() {
        let tree = Arc::new(
            VElement::container(
                "",
                NodeKind::Column,
                vec![
                    VElement::leaf(
                        "0",
                        NodeKind::Text {
                            content: String::from("привет"),
                        },
                    )
                    .with_tag("h2")
                    .with_attr("class", "title"),
                ],
            )
            .with_tag("div"),
        );
        set_tree(active_window(), Arc::clone(&tree));

        assert!(!Dom::is_empty(), "дерево сохранено");
        assert_eq!(Dom::len(), 2, "корень и текстовый узел");
        assert!(Dom::get("0").is_some(), "узел находится по ключу");
        assert_eq!(Dom::nodes().len(), 2);
        assert_eq!(
            Dom::find(|node| matches!(node.kind, NodeKind::Text { .. }))
                .first()
                .map(|node| node.key),
            Some("0")
        );
        assert_eq!(Dom::find_by_tag("h2").map(|node| node.key), Some("0"));
        assert_eq!(Dom::find_by_tag("div").map(|node| node.key), Some(""));
        assert_eq!(
            Dom::find_by_attr("class", "title").map(|node| node.key),
            Some("0")
        );
        assert_eq!(
            Dom::find_by_content("привет").map(|node| node.key),
            Some("0")
        );
        assert_eq!(Dom::find_containing("иве").map(|node| node.key), Some("0"));
        assert_eq!(Dom::find_all(|node| !node.tag.is_empty()).len(), 2);

        remove_tree(active_window());
        assert!(Dom::is_empty(), "дерево удалено при закрытии окна");
    }
}
