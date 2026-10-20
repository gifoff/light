//! Управление фокусом и порядок обхода клавишей `Tab`.
//!
//! В одном окне в фокусе может находиться один узел. Порядок обхода строится
//! по DOM-порядку фокусируемых узлов: узлы с положительным `tabindex` идут
//! первыми в порядке возрастания, затем — все остальные в порядке появления.
//! Узлы с `tabindex="-1"` доступны для программного фокуса, но пропускаются
//! при обходе.

use crate::vdom::VElement;

use super::roles::parse_bool;

/// Отключён ли узел для взаимодействия (флаг `disabled` или `aria-disabled`).
///
/// Тонкая обёртка над [`super::is_disabled`] — единым источником истины.
pub use super::is_disabled;

/// Исключён ли узел (вместе с поддеревом) из дерева доступности.
pub fn is_hidden(element: &VElement) -> bool {
    !element.style.visible
        || element
            .attr("aria-hidden")
            .and_then(super::roles::parse_bool)
            .unwrap_or(false)
}

/// Значение `tabindex` узла, если атрибут задан.
pub fn tabindex(element: &VElement) -> Option<i64> {
    element
        .attr("tabindex")
        .and_then(|value| value.trim().parse().ok())
}

/// Может ли узел получить фокус (в том числе программно).
pub fn is_focusable(element: &VElement) -> bool {
    if is_hidden(element) || is_disabled(element) {
        return false;
    }
    if let Some(explicit) = element.attr("focusable").and_then(parse_bool) {
        return explicit || tabindex(element).is_some_and(|value| value >= 0);
    }
    if let Some(index) = tabindex(element) {
        return index >= 0 || index == -1;
    }
    super::roles::is_focusable_by_role(super::roles::role_for(element))
}

/// Может ли узел быть выбран при обходе клавишей `Tab`.
pub fn is_tabbable(element: &VElement) -> bool {
    if !is_focusable(element) {
        return false;
    }
    !matches!(tabindex(element), Some(index) if index < 0)
}

/// Порядок обхода клавишей `Tab` для поддерева.
///
/// Положительный `tabindex` помещает узел в начало очереди в порядке
/// возрастания; остальные фокусируемые узлы идут в порядке обхода DOM.
/// Узлы с `tabindex="-1"` пропускаются.
pub fn focus_order(root: &VElement) -> Vec<&VElement> {
    let mut candidates: Vec<(usize, i64, &VElement)> = Vec::new();
    collect(root, &mut candidates);
    candidates.sort_by_key(rank);
    candidates
        .into_iter()
        .map(|(_, _, element)| element)
        .collect()
}

fn rank(entry: &(usize, i64, &VElement)) -> (u8, i64, usize) {
    let (index, tab_index, _) = *entry;
    if tab_index > 0 {
        (0, tab_index, index)
    } else {
        (1, 0, index)
    }
}

fn collect<'a>(element: &'a VElement, out: &mut Vec<(usize, i64, &'a VElement)>) {
    if is_hidden(element) {
        return;
    }
    if is_tabbable(element) {
        out.push((out.len(), tabindex(element).unwrap_or(0), element));
    }
    for child in &element.children {
        collect(child, out);
    }
}

/// Ключ первого фокусируемого узла (для `autofocus`).
pub fn first_tabbable(root: &VElement) -> Option<&VElement> {
    focus_order(root).into_iter().next()
}

/// Первый узел, получающий фокус при старте окна (при отсутствии явного).
///
/// Семантика WAI-ARIA: положительный `tabindex` выносит узел вперёд очереди по
/// возрастанию значения, поэтому приоритет отдаётся tabbable-узлу с
/// наименьшим положительным `tabindex` (при равенстве — первый по DOM).
/// Если положительных `tabindex` нет, фокус получает первый tabbable в
/// DOM-порядке. Если tabbable-узлов нет — `None`.
pub fn initial_focus(root: &VElement) -> Option<&VElement> {
    let candidates = focus_order(root);
    candidates
        .iter()
        .filter_map(|element| {
            let index = tabindex(element)?;
            (index > 0).then_some((index, *element))
        })
        .min_by_key(|(index, _)| *index)
        .map(|(_, element)| element)
        .or_else(|| candidates.first().copied())
}

/// Ключ следующего фокусируемого узла после `current` (с зацикливанием).
pub fn next_focus<'a>(root: &'a VElement, current: &str, forward: bool) -> Option<&'a VElement> {
    let order = focus_order(root);
    if order.is_empty() {
        return None;
    }
    let position = order.iter().position(|element| element.key == current);
    let next = match position {
        Some(index) if forward => (index + 1) % order.len(),
        Some(index) => (index + order.len() - 1) % order.len(),
        None if forward => 0,
        None => order.len() - 1,
    };
    order.get(next).copied()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vdom::{ButtonType, NodeKind, Style, VElement};

    fn button(key: &'static str, disabled: bool) -> VElement {
        VElement::leaf(
            key,
            NodeKind::Button {
                disabled,
                button_type: ButtonType::Submit,
            },
        )
        .with_tag("button")
    }

    fn div(key: &'static str, tabindex: Option<&str>, children: Vec<VElement>) -> VElement {
        let mut node = VElement::container(key, NodeKind::Column, children).with_tag("div");
        if let Some(value) = tabindex {
            node.attrs.push(("tabindex", value.to_owned()));
        }
        node
    }

    fn field(key: &'static str, disabled: bool) -> VElement {
        VElement::leaf(
            key,
            NodeKind::TextField {
                value: String::new(),
                placeholder: String::new(),
                disabled,
                multiline: false,
                resize: crate::vdom::Resize::None,
                masked: false,
            },
        )
        .with_tag("text")
    }

    fn keys<'a>(elements: &[&'a VElement]) -> Vec<&'a str> {
        elements.iter().map(|element| element.key).collect()
    }

    #[test]
    fn positive_tabindex_comes_first_in_ascending_order() {
        let root = div(
            "",
            None,
            vec![
                button("a", false),
                field("b", true),
                button("c", false),
                div("d", Some("2"), vec![]),
                div("e", Some("1"), vec![]),
            ],
        );
        assert_eq!(keys(&focus_order(&root)), vec!["e", "d", "a", "c"]);
    }

    #[test]
    fn disabled_and_hidden_nodes_are_skipped() {
        let mut hidden = button("hidden", false);
        hidden.attrs.push(("aria-hidden", "true".to_owned()));
        let root = div(
            "",
            None,
            vec![field("f", true), hidden, button("ok", false)],
        );
        assert_eq!(keys(&focus_order(&root)), vec!["ok"]);
    }

    #[test]
    fn negative_tabindex_is_focusable_but_not_tabbable() {
        let node = div("panel", Some("-1"), vec![]);
        assert!(is_focusable(&node));
        assert!(!is_tabbable(&node));
    }

    #[test]
    fn next_focus_wraps_around() {
        let root = div("", None, vec![button("a", false), button("b", false)]);
        assert_eq!(next_focus(&root, "a", true).map(|e| e.key), Some("b"));
        assert_eq!(next_focus(&root, "b", true).map(|e| e.key), Some("a"));
        assert_eq!(next_focus(&root, "a", false).map(|e| e.key), Some("b"));
    }

    #[test]
    fn first_tabbable_finds_leading_positive() {
        let root = div(
            "",
            None,
            vec![button("a", false), div("z", Some("3"), vec![])],
        );
        assert_eq!(first_tabbable(&root).map(|e| e.key), Some("z"));
    }

    #[test]
    fn focusable_attribute_overrides_role() {
        let mut node = div("panel", None, vec![]);
        node.attrs.push(("focusable", "true".to_owned()));
        assert!(is_focusable(&node));

        let mut node = button("btn", false);
        node.attrs.push(("focusable", "false".to_owned()));
        assert!(!is_focusable(&node));
    }

    #[test]
    fn style_visibility_excludes_nodes() {
        let mut node = button("a", false);
        node.style = Style {
            visible: false,
            ..Style::default()
        };
        assert!(is_hidden(&node));
        assert!(!is_focusable(&node));
    }
}
