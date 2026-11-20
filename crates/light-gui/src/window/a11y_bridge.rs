//! Интеграция с AccessKit: дерево доступности, действия диктора и клавиатура.

#[cfg(feature = "a11y-winit")]
use light_vdom::Event as DomEvent;
#[cfg(feature = "a11y-winit")]
use light_vdom::NodeKind;
#[cfg(feature = "a11y-winit")]
use winit::event::KeyEvent;

#[cfg(feature = "a11y-winit")]
use super::keys::key_info;
#[cfg(feature = "a11y-winit")]
use super::state::WindowState;

#[cfg(feature = "a11y-winit")]
impl WindowState {
    /// Отправляет текущее дерево доступности адаптеру (если тот активен).
    pub(super) fn push_accessibility(&mut self) {
        let Some(adapter) = self.adapter.as_mut() else {
            return;
        };
        let Some(tree) = self.tree.clone() else {
            return;
        };
        let focus = self.a11y_focus.clone().or_else(|| self.focused.clone());
        #[cfg(feature = "a11y-live")]
        let _ = crate::a11y::take_announcements();
        adapter.update_if_active(|| crate::a11y::tree::build_update(&tree, focus.as_deref()));
    }

    /// Выполняет действие, запрошенное вспомогательной технологией.
    pub(super) fn handle_action_request(&mut self, request: &accesskit::ActionRequest) {
        let node = request.target_node;
        let Some(translated) = crate::a11y::actions::translate(request) else {
            return;
        };
        let key = self
            .tree
            .as_ref()
            .and_then(|tree| crate::a11y::tree::find_key_for_node_id(tree, node))
            .map(str::to_owned);
        match translated {
            crate::a11y::actions::A11yRequest::Click => {
                if let Some(key) = &key {
                    self.dispatch_checked(key, DomEvent::Click);
                }
            }
            crate::a11y::actions::A11yRequest::SetValue(value) => {
                if let Some(key) = &key {
                    self.dispatch_checked(key, DomEvent::TextChange(value));
                }
            }
            crate::a11y::actions::A11yRequest::Focus => self.set_a11y_focus(key),
            crate::a11y::actions::A11yRequest::Blur => self.set_a11y_focus(None),
            crate::a11y::actions::A11yRequest::Increment
            | crate::a11y::actions::A11yRequest::Decrement
            | crate::a11y::actions::A11yRequest::Expand
            | crate::a11y::actions::A11yRequest::Collapse => {}
        }
        self.invalidate();
    }

    /// Фиксирует фокус доступности и синхронизирует его с полем ввода.
    pub(super) fn set_a11y_focus(&mut self, key: Option<String>) {
        let is_text = key.as_deref().is_some_and(|key| {
            self.tree
                .as_ref()
                .and_then(|tree| tree.get(key))
                .is_some_and(|element| {
                    matches!(
                        element.kind,
                        NodeKind::TextField {
                            disabled: false,
                            ..
                        }
                    )
                })
        });
        if is_text {
            self.focused = key.clone();
        } else {
            self.focused = None;
        }
        self.a11y_focus = key.clone();
        crate::a11y::set_focused_node(key.as_deref().map(crate::a11y::tree::node_id_for_key));
    }

    /// Переводит фокус на следующий либо предыдущий фокусируемый узел.
    fn move_a11y_focus(&mut self, forward: bool) -> bool {
        let Some(tree) = self.tree.clone() else {
            return false;
        };
        let current = self.a11y_focus.clone().or_else(|| self.focused.clone());
        let next = match &current {
            Some(key) => crate::a11y::focus::next_focus(&tree, key, forward),
            None => crate::a11y::focus::first_tabbable(&tree),
        };
        let Some(next) = next else {
            return false;
        };
        self.set_a11y_focus(Some(next.key.to_owned()));
        self.invalidate();
        true
    }

    /// Переносит фокус доступности на следующий tabbable-узел, если узел под
    /// фокусом исчез, стал скрытым или отключённым.
    ///
    /// Без этого после повторной отрисовки диктор оставался бы на недоступном
    /// элементе (например, на кнопке, которая только что стала `disabled`).
    pub(super) fn reconcile_a11y_focus(&mut self) {
        let Some(tree) = self.tree.clone() else {
            return;
        };
        let Some(key) = self.a11y_focus.clone() else {
            return;
        };
        let unavailable = tree.get(&key).is_none_or(|element| {
            crate::a11y::focus::is_hidden(element) || crate::a11y::is_disabled(element)
        });
        if !unavailable {
            return;
        }
        let next = crate::a11y::focus::next_focus(&tree, &key, true)
            .or_else(|| crate::a11y::focus::first_tabbable(&tree))
            .map(|element| element.key.to_owned());
        self.set_a11y_focus(next);
    }

    /// Активирует узел в фокусе клавишами `Enter`/`Space`.
    fn activate_a11y_focus(&mut self) -> bool {
        let key = self.a11y_focus.clone().or_else(|| self.focused.clone());
        let Some(key) = key else {
            return false;
        };
        if self.node_disabled(&key) {
            return false;
        }
        let is_text = self
            .tree
            .as_ref()
            .and_then(|tree| tree.get(&key))
            .is_some_and(|element| matches!(element.kind, NodeKind::TextField { .. }));
        if is_text {
            return false;
        }
        self.dispatch_checked(&key, DomEvent::Click);
        self.invalidate();
        true
    }

    /// Обрабатывает встроенные клавиатурные сокращения (`Tab`, `Enter`, …).
    ///
    /// Возвращает `true`, если клавиша обработана и не должна попадать
    /// в логику редактирования текста.
    #[cfg(feature = "a11y-keyboard")]
    pub(super) fn handle_builtin_key(&mut self, event: &KeyEvent) -> bool {
        if !crate::a11y::is_enabled() {
            return false;
        }
        let info = key_info(event, self.modifiers);
        let Some(action) = crate::a11y::keyboard::builtin_action(&info.key, info.shift) else {
            return false;
        };
        use crate::a11y::keyboard::KeyAction;
        match action {
            KeyAction::NextFocus => self.move_a11y_focus(true),
            KeyAction::PrevFocus => self.move_a11y_focus(false),
            KeyAction::Activate => self.activate_a11y_focus(),
            KeyAction::Escape | KeyAction::MovePrev | KeyAction::MoveNext => false,
        }
    }

    #[cfg(not(feature = "a11y-keyboard"))]
    pub(super) fn handle_builtin_key(&mut self, _event: &KeyEvent) -> bool {
        false
    }
}
