//! Построение дерева доступности ([`accesskit::TreeUpdate`]) из узлов Light.
//!
//! Обход выполняется в порядке DOM; каждый узел получает стабильный
//! [`NodeId`], выведенный из пути [`VElement::key`], поэтому между
//! перерисовками идентификаторы не меняются. Поддеревья с `aria-hidden="true"`
//! или `visible="false"` в дерево не попадают.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use accesskit::{Invalid, Node, NodeId, Role, Toggled, TreeId, TreeInfo, TreeUpdate};

#[cfg(feature = "a11y-live")]
use super::Politeness;
use super::focus;
use super::roles;
use crate::vdom::{NodeKind, VElement};

/// Стабильный [`NodeId`] для пути узла `key`.
///
/// Одинаковые пути получают один и тот же идентификатор на всё время работы
/// приложения, что позволяет AccessKit обновлять дерево инкрементально.
pub fn node_id_for_key(key: &str) -> NodeId {
    fn pool() -> &'static Mutex<Pool> {
        static POOL: OnceLock<Mutex<Pool>> = OnceLock::new();
        POOL.get_or_init(|| Mutex::new(Pool::default()))
    }
    let mut pool = pool()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(id) = pool.ids.get(key) {
        return NodeId(*id);
    }
    pool.next += 1;
    let id = pool.next;
    pool.ids.insert(key.to_owned(), id);
    NodeId(id)
}

#[derive(Default)]
struct Pool {
    ids: HashMap<String, u64>,
    next: u64,
}

/// Ключ узла по идентификатору AccessKit (обратный поиск).
///
/// Используется оконным бэкендом, чтобы сопоставить [`accesskit::ActionRequest`]
/// с узлом виртуального дерева. Возвращает `None`, если узел не найден.
pub fn find_key_for_node_id(root: &VElement, id: NodeId) -> Option<&'static str> {
    if node_id_for_key(root.key) == id {
        return Some(root.key);
    }
    root.children
        .iter()
        .find_map(|child| find_key_for_node_id(child, id))
}

/// Строит [`TreeUpdate`] для поддерева `root`.
///
/// `focused_key` — путь узла, который должен быть помечен как находящийся
/// в фокусе. Если ключ не задан или узел не найден, фокус получает первый
/// узел по правилам [`focus::initial_focus`] (наименьший положительный
/// `tabindex`, иначе первый tabbable в DOM-порядке), а при отсутствии
/// tabbable-узлов — корень.
pub fn build_update(root: &VElement, focused_key: Option<&str>) -> TreeUpdate {
    let mut id_map: HashMap<&str, NodeId> = HashMap::new();
    collect_ids(root, &mut id_map);

    let mut nodes: Vec<(NodeId, Node)> = Vec::new();
    let root_id = build_subtree(root, &id_map, &mut nodes);
    let focus = focused_key
        .map(node_id_for_key)
        .filter(|id| nodes.iter().any(|(node, _)| node == id))
        .or_else(|| {
            focus::initial_focus(root)
                .map(|element| node_id_for_key(element.key))
                .filter(|id| nodes.iter().any(|(node, _)| node == id))
        })
        .unwrap_or(root_id);

    TreeUpdate {
        nodes,
        tree: Some(TreeInfo::new(root_id)),
        tree_id: TreeId::ROOT,
        focus,
    }
}

fn collect_ids<'a>(element: &'a VElement, out: &mut HashMap<&'a str, NodeId>) {
    if let Some(id) = element.attr("id")
        && !id.is_empty()
    {
        out.entry(id)
            .or_insert_with(|| node_id_for_key(element.key));
    }
    for child in &element.children {
        collect_ids(child, out);
    }
}

fn build_subtree<'a>(
    element: &'a VElement,
    id_map: &HashMap<&'a str, NodeId>,
    nodes: &mut Vec<(NodeId, Node)>,
) -> NodeId {
    let mut child_ids = Vec::new();
    for child in &element.children {
        if focus::is_hidden(child) {
            continue;
        }
        child_ids.push(build_subtree(child, id_map, nodes));
    }
    let id = node_id_for_key(element.key);
    let node = build_node(element, id_map, child_ids);
    nodes.push((id, node));
    id
}

fn build_node(element: &VElement, id_map: &HashMap<&str, NodeId>, children: Vec<NodeId>) -> Node {
    let role = roles::role_for(element);
    let mut node = Node::new(role);
    apply_label(&mut node, element, id_map);
    apply_description(&mut node, element, id_map);
    apply_state(&mut node, element);
    apply_value(&mut node, element);
    if let Some(level) = roles::heading_level(element.tag) {
        node.set_level(level);
    }
    for action in super::actions::default_actions(element) {
        node.add_action(action);
    }
    node.set_children(children);
    node
}

fn apply_label(node: &mut Node, element: &VElement, id_map: &HashMap<&str, NodeId>) {
    if let Some(label) = element.attr("aria-label") {
        if !label.is_empty() {
            node.set_label(label);
        }
        return;
    }
    if let Some(ids) = resolve_refs(element.attr("aria-labelledby"), id_map) {
        node.set_labelled_by(ids);
        return;
    }
    if let Some(name) = intrinsic_name(element) {
        node.set_label(name);
    }
}

fn apply_description(node: &mut Node, element: &VElement, id_map: &HashMap<&str, NodeId>) {
    if let Some(ids) = resolve_refs(element.attr("aria-describedby"), id_map) {
        node.set_described_by(ids);
    }
    if let Some(text) = element.attr("aria-description")
        && !text.is_empty()
    {
        node.set_description(text);
    }
}

fn apply_state(node: &mut Node, element: &VElement) {
    if focus::is_disabled(element) {
        node.set_disabled();
    }
    if let Some(expanded) = element.attr("aria-expanded").and_then(roles::parse_bool) {
        node.set_expanded(expanded);
    }
    if let Some(toggled) = checked_state(element) {
        node.set_toggled(toggled);
    }
    if let Some(selected) = element.attr("aria-selected").and_then(roles::parse_bool) {
        node.set_selected(selected);
    }
    if is_required(element) {
        node.set_required();
    }
    if is_invalid(element) {
        node.set_invalid(Invalid::True);
    }
    #[cfg(feature = "a11y-live")]
    if let Some(live) = element.attr("aria-live").and_then(Politeness::parse) {
        node.set_live(live.to_live());
    }
    if let Some(min) = element.attr("aria-valuemin").and_then(parse_f64) {
        node.set_min_numeric_value(min);
    }
    if let Some(max) = element.attr("aria-valuemax").and_then(parse_f64) {
        node.set_max_numeric_value(max);
    }
    if let Some(now) = element.attr("aria-valuenow").and_then(parse_f64) {
        node.set_numeric_value(now);
    }
}

fn apply_value(node: &mut Node, element: &VElement) {
    match &element.kind {
        NodeKind::TextField {
            value, placeholder, ..
        } => {
            node.set_value(value.as_str());
            if !placeholder.is_empty() {
                node.set_placeholder(placeholder.as_str());
            }
        }
        #[cfg(feature = "elements-select")]
        NodeKind::Select { value, .. } => node.set_value(value.as_str()),
        _ => {}
    }
}

fn checked_state(element: &VElement) -> Option<Toggled> {
    if let Some(toggled) = element.attr("aria-checked").and_then(roles::parse_tristate) {
        return Some(toggled);
    }
    match &element.kind {
        #[cfg(feature = "elements-checkbox")]
        NodeKind::Checkbox { checked, .. } => Some(Toggled::from(*checked)),
        #[cfg(feature = "elements-radio")]
        NodeKind::Radio { selected, .. } => Some(Toggled::from(*selected)),
        _ => None,
    }
}

fn is_required(element: &VElement) -> bool {
    element
        .attr("aria-required")
        .and_then(roles::parse_bool)
        .or_else(|| element.attr("required").and_then(roles::parse_bool))
        .unwrap_or(false)
}

fn is_invalid(element: &VElement) -> bool {
    element
        .attr("aria-invalid")
        .and_then(roles::parse_bool)
        .or_else(|| element.attr("invalid").and_then(roles::parse_bool))
        .unwrap_or(false)
}

fn parse_f64(value: &str) -> Option<f64> {
    value.trim().parse().ok()
}

fn resolve_refs(value: Option<&str>, id_map: &HashMap<&str, NodeId>) -> Option<Vec<NodeId>> {
    let ids: Vec<NodeId> = value?
        .split_whitespace()
        .filter_map(|id| id_map.get(id).copied())
        .collect();
    (!ids.is_empty()).then_some(ids)
}

fn collected_text(element: &VElement) -> String {
    let mut text = String::new();
    collect_text_into(element, &mut text);
    text.trim().to_owned()
}

fn collect_text_into(element: &VElement, out: &mut String) {
    if let NodeKind::Text { content } = &element.kind {
        out.push_str(content);
        return;
    }
    for child in &element.children {
        collect_text_into(child, out);
    }
}

fn intrinsic_name(element: &VElement) -> Option<String> {
    match &element.kind {
        #[cfg(feature = "elements-checkbox")]
        NodeKind::Checkbox { label, .. } if !label.is_empty() => Some(label.clone()),
        #[cfg(feature = "elements-radio")]
        NodeKind::Radio { label, .. } if !label.is_empty() => Some(label.clone()),
        NodeKind::Text { content } if !content.is_empty() => Some(content.clone()),
        #[cfg(feature = "image-support")]
        NodeKind::Image { alt, .. } if !alt.is_empty() => Some(alt.clone()),
        #[cfg(feature = "image-support")]
        NodeKind::RuntimeImage { alt, .. } if !alt.is_empty() => Some(alt.clone()),
        NodeKind::Window => element.titlebar_text(),
        NodeKind::TextField { placeholder, .. } if !placeholder.is_empty() => {
            Some(placeholder.clone())
        }
        _ => {
            if should_derive_label_from_children(roles::role_for(element)) {
                let text = collected_text(element);
                (!text.is_empty()).then_some(text)
            } else {
                None
            }
        }
    }
}

/// Роли, для которых подпись из текста детей является частью семантики.
///
/// Для контейнеров (`GenericContainer`, `Window`, `Group`, `List` и т. п.)
/// имя заполняется только явным `aria-label`/`aria-labelledby`, иначе
/// скринридер дублировал бы содержимое всего поддерева.
fn should_derive_label_from_children(role: Role) -> bool {
    matches!(
        role,
        Role::Button
            | Role::Label
            | Role::Link
            | Role::Heading
            | Role::CheckBox
            | Role::RadioButton
            | Role::MenuItem
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vdom::{NodeKind, VElement};
    use accesskit::{NodeId, Role};

    fn text(key: &'static str, content: &str) -> VElement {
        VElement::leaf(
            key,
            NodeKind::Text {
                content: content.to_owned(),
            },
        )
    }

    fn button(key: &'static str, label: &str) -> VElement {
        VElement::container(
            key,
            NodeKind::Button {
                disabled: false,
                button_type: crate::vdom::ButtonType::Submit,
            },
            vec![text("t", label)],
        )
        .with_tag("button")
    }

    fn heading(key: &'static str, content: &str) -> VElement {
        VElement::container(key, NodeKind::Column, vec![text("t", content)]).with_tag("h1")
    }

    fn find(update: &TreeUpdate, role: Role) -> Option<&Node> {
        update
            .nodes
            .iter()
            .find(|(_, node)| node.role() == role)
            .map(|(_, node)| node)
    }

    #[test]
    fn roles_labels_and_levels_are_built() {
        let root = VElement::container(
            "",
            NodeKind::Column,
            vec![heading("h", "Title"), button("b", "OK")],
        )
        .with_tag("div");
        let update = build_update(&root, None);

        let button = find(&update, Role::Button).expect("button node");
        assert_eq!(button.label(), Some("OK"));

        let heading = find(&update, Role::Heading).expect("heading node");
        assert_eq!(heading.level(), Some(1));
        assert_eq!(heading.label(), Some("Title"));
    }

    #[test]
    fn aria_hidden_subtree_is_excluded() {
        let mut hidden = button("hidden", "Hidden");
        hidden.attrs.push(("aria-hidden", "true".to_owned()));
        let root =
            VElement::container("", NodeKind::Column, vec![button("shown", "Shown"), hidden])
                .with_tag("div");
        let update = build_update(&root, None);
        assert_eq!(update.nodes.len(), 3);
        assert!(find(&update, Role::Button).is_some());
    }

    #[test]
    fn aria_labelledby_links_target_node() {
        let mut field = VElement::leaf(
            "f",
            NodeKind::TextField {
                value: String::new(),
                placeholder: String::new(),
                disabled: false,
                multiline: false,
                resize: crate::vdom::Resize::None,
                masked: false,
            },
        )
        .with_tag("text");
        field.attrs.push(("aria-labelledby", "lbl".to_owned()));
        let mut label = text("lbl", "Name");
        label.attrs.push(("id", "lbl".to_owned()));
        let root = VElement::container("", NodeKind::Column, vec![label, field]).with_tag("div");

        let update = build_update(&root, None);
        let text_input = find(&update, Role::TextInput).expect("text input");
        let label_id = node_id_for_key("lbl");
        assert_eq!(text_input.labelled_by(), &[label_id]);
    }

    #[test]
    #[cfg(feature = "elements-checkbox")]
    fn checkbox_state_is_reported() {
        let checked = VElement::leaf(
            "c1",
            NodeKind::Checkbox {
                label: "A".to_owned(),
                checked: true,
                disabled: false,
            },
        )
        .with_tag("checkbox");
        let unchecked = VElement::leaf(
            "c2",
            NodeKind::Checkbox {
                label: "B".to_owned(),
                checked: false,
                disabled: false,
            },
        )
        .with_tag("checkbox");
        let root =
            VElement::container("", NodeKind::Column, vec![checked, unchecked]).with_tag("div");
        let update = build_update(&root, None);
        let states: Vec<Toggled> = update
            .nodes
            .iter()
            .filter(|(_, node)| node.role() == Role::CheckBox)
            .filter_map(|(_, node)| node.toggled())
            .collect();
        assert_eq!(states, vec![Toggled::True, Toggled::False]);
    }

    #[test]
    fn focus_points_at_requested_key() {
        let root = VElement::container(
            "",
            NodeKind::Column,
            vec![button("a", "A"), button("b", "B")],
        )
        .with_tag("div");
        let update = build_update(&root, Some("b"));
        assert_eq!(update.focus, node_id_for_key("b"));
    }

    #[test]
    fn window_label_comes_from_titlebar() {
        let root = VElement::container(
            "",
            NodeKind::Window,
            vec![VElement::container(
                "tb",
                NodeKind::TitleBar,
                vec![text("t", "Settings")],
            )],
        )
        .with_tag("window");
        let update = build_update(&root, None);
        let window = find(&update, Role::Window).expect("window node");
        assert_eq!(window.label(), Some("Settings"));
    }

    #[test]
    fn ids_are_stable_across_builds() {
        let root =
            VElement::container("", NodeKind::Column, vec![button("a", "A")]).with_tag("div");
        let first = build_update(&root, None);
        let second = build_update(&root, None);
        assert_eq!(first.nodes, second.nodes);
    }

    #[test]
    fn reverse_lookup_finds_key_by_node_id() {
        let root =
            VElement::container("", NodeKind::Column, vec![button("b", "OK")]).with_tag("div");
        assert_eq!(find_key_for_node_id(&root, node_id_for_key("b")), Some("b"));
        assert_eq!(find_key_for_node_id(&root, NodeId(9_999_999)), None);
    }
}
