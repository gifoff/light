//! Обработка действий, запрошенных вспомогательными технологиями.
//!
//! Экранный диктор может попросить выполнить действие над узлом (`Click`,
//! `Focus`, `SetValue`, …). AccessKit присылает [`ActionRequest`], который
//! превращается в [`A11yRequest`] — нейтральное представление, которое оконный
//! бэкенд превращает в событие Relay или в команду фокуса.

use accesskit::ActionRequest;

use super::roles::{is_focusable_by_role, role_for};
use crate::vdom::{Event, NodeKind, VElement};

/// Действие доступности, адресованное конкретному узлу.
#[derive(Debug, Clone, PartialEq)]
pub enum A11yRequest {
    /// Нажать на узел (эмуляция клика).
    Click,
    /// Перевести фокус на узел.
    Focus,
    /// Снять фокус.
    Blur,
    /// Установить значение поля (`v-model`).
    SetValue(String),
    /// Увеличить значение на шаг.
    Increment,
    /// Уменьшить значение на шаг.
    Decrement,
    /// Раскрыть узел.
    Expand,
    /// Свернуть узел.
    Collapse,
}

impl A11yRequest {
    /// Событие Relay, соответствующее действию, если действие —
    /// пользовательское событие.
    pub fn to_event(&self) -> Option<Event> {
        match self {
            A11yRequest::Click => Some(Event::Click),
            A11yRequest::SetValue(value) => Some(Event::TextChange(value.clone())),
            _ => None,
        }
    }
}

/// Набор действий AccessKit, поддерживаемых узлом.
pub fn default_actions(element: &VElement) -> Vec<accesskit::Action> {
    use accesskit::Action;
    let mut actions = Vec::new();
    let role = role_for(element);
    if is_focusable_by_role(role) {
        actions.push(Action::Focus);
    }
    match &element.kind {
        NodeKind::Button { .. } | NodeKind::Checkbox { .. } | NodeKind::Radio { .. } => {
            actions.push(Action::Click);
        }
        NodeKind::TextField { .. } | NodeKind::Select { .. } => {
            actions.push(Action::SetValue);
        }
        _ => {}
    }
    if matches!(
        role,
        accesskit::Role::CheckBox | accesskit::Role::RadioButton | accesskit::Role::Switch
    ) {
        actions.push(Action::Click);
    }
    actions
}

/// Преобразует запрос AccessKit в действие Relay.
///
/// Возвращает `None`, если действие не поддерживается.
pub fn translate(request: &ActionRequest) -> Option<A11yRequest> {
    use accesskit::Action;
    let value = request.data.as_ref();
    match request.action {
        Action::Click => Some(A11yRequest::Click),
        Action::Focus | Action::SetSequentialFocusNavigationStartingPoint => {
            Some(A11yRequest::Focus)
        }
        Action::Blur => Some(A11yRequest::Blur),
        Action::Increment => Some(A11yRequest::Increment),
        Action::Decrement => Some(A11yRequest::Decrement),
        Action::Expand => Some(A11yRequest::Expand),
        Action::Collapse => Some(A11yRequest::Collapse),
        Action::SetValue | Action::ReplaceSelectedText => match value {
            Some(accesskit::ActionData::Value(text)) => {
                Some(A11yRequest::SetValue(text.to_string()))
            }
            Some(accesskit::ActionData::NumericValue(number)) => {
                Some(A11yRequest::SetValue(number.to_string()))
            }
            _ => None,
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use accesskit::{ActionData, NodeId, TreeId};

    fn button() -> VElement {
        VElement::leaf(
            "b",
            NodeKind::Button {
                label: "OK".to_owned(),
                disabled: false,
            },
        )
        .with_tag("button")
    }

    fn text_field() -> VElement {
        VElement::leaf(
            "t",
            NodeKind::TextField {
                value: "hi".to_owned(),
                placeholder: String::new(),
                disabled: false,
                multiline: false,
                resize: crate::vdom::Resize::None,
                masked: false,
            },
        )
        .with_tag("text")
    }

    fn request(action: accesskit::Action, data: Option<accesskit::ActionData>) -> ActionRequest {
        ActionRequest {
            action,
            target_tree: TreeId::ROOT,
            target_node: NodeId(1),
            data,
        }
    }

    #[test]
    fn buttons_and_fields_advertise_actions() {
        let actions = default_actions(&button());
        assert!(actions.contains(&accesskit::Action::Click));
        assert!(actions.contains(&accesskit::Action::Focus));

        let actions = default_actions(&text_field());
        assert!(actions.contains(&accesskit::Action::SetValue));
        assert!(!actions.contains(&accesskit::Action::Click));
    }

    #[test]
    fn click_and_value_requests_translate() {
        assert_eq!(
            translate(&request(accesskit::Action::Click, None)),
            Some(A11yRequest::Click)
        );
        assert_eq!(
            translate(&request(
                accesskit::Action::SetValue,
                Some(ActionData::Value("new".into()))
            )),
            Some(A11yRequest::SetValue("new".to_owned()))
        );
    }

    #[test]
    fn unsupported_requests_are_ignored() {
        assert_eq!(
            translate(&request(accesskit::Action::ShowContextMenu, None)),
            None
        );
        assert_eq!(translate(&request(accesskit::Action::SetValue, None)), None);
    }

    #[test]
    fn requests_map_to_relay_events() {
        assert_eq!(A11yRequest::Click.to_event(), Some(Event::Click));
        assert_eq!(
            A11yRequest::SetValue("z".to_owned()).to_event(),
            Some(Event::TextChange("z".to_owned()))
        );
        assert_eq!(A11yRequest::Focus.to_event(), None);
    }
}
