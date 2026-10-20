//! Встроенные клавиатурные сокращения.
//!
//! Эти действия оконный бэкенд выполняет **до** пользовательских
//! обработчиков `@keydown`/`@keyup`: если обработчик вернёт `true`, он
//! переопределяет встроенное поведение.
//!
//! * `Tab` / `Shift+Tab` — переход по фокусируемым узлам;
//! * `Enter` / `Space` — активация кнопки, флажка или переключателя;
//! * `Escape` — закрытие модального окна или раскрытого списка;
//! * стрелки — навигация внутри группы переключателей и списка.

/// Кнопка отправки ближайшей формы — см. [`crate::validation::find_submit_button`].
pub use crate::validation::find_submit_button;

/// Встроенное действие, соответствующее нажатой клавише.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyAction {
    /// Перейти к следующему фокусируемому узлу (`Tab`).
    NextFocus,
    /// Перейти к предыдущему фокусируемому узлу (`Shift+Tab`).
    PrevFocus,
    /// Активировать текущий узел (`Enter`/`Space`).
    Activate,
    /// Закрыть модальное окно или раскрытый список (`Escape`).
    Escape,
    /// Выбрать предыдущий элемент (`ArrowUp`/`ArrowLeft`).
    MovePrev,
    /// Выбрать следующий элемент (`ArrowDown`/`ArrowRight`).
    MoveNext,
}

/// Встроенное действие для клавиши `key` с учётом `Shift`.
///
/// Значения `key` совпадают с [`crate::vdom::KeyInfo::key`] (`"Tab"`,
/// `"Enter"`, `" "`, `"Escape"`, `"ArrowUp"`…).
pub fn builtin_action(key: &str, shift: bool) -> Option<KeyAction> {
    let action = match key {
        "Tab" if shift => KeyAction::PrevFocus,
        "Tab" => KeyAction::NextFocus,
        "Enter" | " " | "Space" => KeyAction::Activate,
        "Escape" => KeyAction::Escape,
        "ArrowUp" | "ArrowLeft" => KeyAction::MovePrev,
        "ArrowDown" | "ArrowRight" => KeyAction::MoveNext,
        _ => return None,
    };
    Some(action)
}

/// Входит ли клавиша во встроенную обработку.
pub fn is_builtin(key: &str, shift: bool) -> bool {
    builtin_action(key, shift).is_some()
}

/// Клавиша активации (`Enter`/`Space`).
pub fn is_activation_key(key: &str) -> bool {
    matches!(key, "Enter" | " " | "Space")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_moves_in_both_directions() {
        assert_eq!(builtin_action("Tab", false), Some(KeyAction::NextFocus));
        assert_eq!(builtin_action("Tab", true), Some(KeyAction::PrevFocus));
    }

    #[test]
    fn enter_and_space_activate() {
        assert_eq!(builtin_action("Enter", false), Some(KeyAction::Activate));
        assert_eq!(builtin_action(" ", false), Some(KeyAction::Activate));
        assert!(is_activation_key("Enter"));
        assert!(!is_activation_key("Tab"));
    }

    #[test]
    fn escape_and_arrows_are_recognised() {
        assert_eq!(builtin_action("Escape", false), Some(KeyAction::Escape));
        assert_eq!(builtin_action("ArrowUp", false), Some(KeyAction::MovePrev));
        assert_eq!(
            builtin_action("ArrowDown", false),
            Some(KeyAction::MoveNext)
        );
        assert_eq!(builtin_action("a", false), None);
    }
}
