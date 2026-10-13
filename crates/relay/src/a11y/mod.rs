//! Доступность интерфейса (Accessibility / a11y).
//!
//! Модуль описывает, как виртуальное дерево [`crate::vdom::VElement`]
//! превращается в дерево доступности [AccessKit](https://accesskit.dev/),
//! которое читают экранные дикторы (NVDA, Narrator, VoiceOver, Orca).
//!
//! ## Состав
//!
//! * [`roles`] — сопоставление HTML-подобных тегов Relay и атрибута
//!   `role="…"` с [`AriaRole`](accesskit::Role);
//! * [`focus`] — фокусируемые узлы и порядок обхода клавишей `Tab`;
//! * [`tree`] — построение [`accesskit::TreeUpdate`] из дерева узлов;
//! * [`actions`] — преобразование запросов AccessKit (`click`, `focus`,
//!   `set-value`) в события Relay;
//! * [`keyboard`] — встроенные клавиатурные сокращения (Tab, Enter, Space,
//!   Escape, стрелки).
//!
//! ## Отключение
//!
//! Встроенную поддержку можно выключить во время исполнения переменной
//! окружения `RELAY_A11Y=0` или программно — [`set_enabled`]. При сборке без
//! фичи `a11y` все функции-аксессоры возвращают `None`, а атрибуты шаблонов
//! просто игнорируются.

use std::cell::{Cell, RefCell};

#[cfg(feature = "a11y")]
pub use accesskit::Role as AriaRole;

/// Обратная совместимость: без фичи `a11y` роль — минимальная заглушка.
#[cfg(not(feature = "a11y"))]
pub use fallback::AriaRole;

#[cfg(feature = "a11y")]
pub use accesskit::NodeId;

#[cfg(not(feature = "a11y"))]
pub use fallback::NodeId;

#[cfg(feature = "a11y")]
pub mod actions;
#[cfg(feature = "a11y")]
pub mod focus;
#[cfg(feature = "a11y")]
pub mod keyboard;
#[cfg(feature = "a11y")]
pub mod roles;
#[cfg(feature = "a11y")]
pub mod tree;

/// Заглушки публичного API, доступные при выключенной фиче `a11y`.
#[cfg(not(feature = "a11y"))]
pub mod fallback {
    /// Непрозрачный идентификатор узла дерева доступности.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct NodeId(pub u64);

    /// Минимальная заглушка роли — фича `a11y` выключена.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    #[non_exhaustive]
    pub enum AriaRole {
        /// Роль не определена.
        #[default]
        Unknown,
    }
}

/// Разбирает булев атрибут (`true`/`false`; пустое значение — `true`).
pub fn parse_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "" | "true" | "1" | "yes" => Some(true),
        "false" | "0" | "no" => Some(false),
        _ => None,
    }
}

/// Отключён ли узел для взаимодействия (`disabled` / `aria-disabled`).
///
/// Единый источник истины для блокировки кликов, клавиатурной активации и
/// курсора. Атрибут `aria-disabled` имеет приоритет; иначе смотрится
/// встроенный флаг `disabled` у интерактивных узлов. Доступна независимо от
/// фичи `a11y`, поэтому проверку можно использовать в оконной логике.
pub fn is_disabled(element: &crate::vdom::VElement) -> bool {
    use crate::vdom::NodeKind;
    if element.attr("aria-disabled").and_then(parse_bool) == Some(true) {
        return true;
    }
    match &element.kind {
        NodeKind::Button { disabled, .. }
        | NodeKind::TextField { disabled, .. }
        | NodeKind::Checkbox { disabled, .. }
        | NodeKind::Radio { disabled, .. }
        | NodeKind::Select { disabled, .. } => *disabled,
        _ => false,
    }
}

/// Срочность объявления изменений live-региона (`aria-live`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Politeness {
    /// Не объявлять (`off`).
    Off,
    /// Объявлять, дождавшись паузы в работе диктора (`polite`, по умолчанию).
    #[default]
    Polite,
    /// Объявлять немедленно, прерывая диктора (`assertive`).
    Assertive,
}

#[cfg(feature = "a11y")]
impl Politeness {
    /// Преобразует срочность в тип [`accesskit::Live`].
    pub fn to_live(self) -> accesskit::Live {
        match self {
            Politeness::Off => accesskit::Live::Off,
            Politeness::Polite => accesskit::Live::Polite,
            Politeness::Assertive => accesskit::Live::Assertive,
        }
    }

    /// Разбирает значение атрибута `aria-live`.
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "off" => Some(Politeness::Off),
            "polite" => Some(Politeness::Polite),
            "assertive" => Some(Politeness::Assertive),
            _ => None,
        }
    }
}

/// Очередная просьба диктору что-то объявить.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Announcement {
    /// Текст объявления.
    pub message: String,
    /// Срочность объявления.
    pub politeness: Politeness,
}

thread_local! {
    /// Включена ли поддержка доступности в текущем потоке.
    static ENABLED: Cell<bool> = Cell::new(env_default());
    /// Узел, находящийся в фокусе (идентификатор AccessKit).
    static FOCUSED: Cell<Option<NodeId>> = const { Cell::new(None) };
    /// Очередь объявлений, которые ещё не забрал оконный бэкенд.
    static ANNOUNCEMENTS: RefCell<Vec<Announcement>> = const { RefCell::new(Vec::new()) };
}

/// Значение по умолчанию для [`is_enabled`]: учитывает `RELAY_A11Y=0`.
fn env_default() -> bool {
    std::env::var("RELAY_A11Y")
        .map(|value| value.trim() != "0")
        .unwrap_or(true)
}

/// Включает или выключает встроенную поддержку AccessKit.
///
/// Удобно для тестов и отладки. Не влияет на разбор атрибутов шаблонов —
/// при выключенной поддержке они лишь игнорируются.
pub fn set_enabled(enabled: bool) {
    ENABLED.with(|flag| flag.set(enabled));
}

/// Включена ли поддержка доступности в текущем потоке.
///
/// Возвращает `false`, если задана переменная окружения `RELAY_A11Y=0`.
pub fn is_enabled() -> bool {
    ENABLED.with(Cell::get)
}

/// Уведомляет диктора об изменении live-региона.
///
/// Объявление ставится в очередь и будет отправлено оконным бэкендом при
/// следующей перерисовке. Если поддержка выключена, вызов ничего не делает.
pub fn announce(message: impl Into<String>, politeness: Politeness) {
    if !is_enabled() {
        return;
    }
    let announcement = Announcement {
        message: message.into(),
        politeness,
    };
    ANNOUNCEMENTS.with(|queue| queue.borrow_mut().push(announcement));
}

/// Забирает все накопленные объявления (вызывает оконный бэкенд).
pub fn take_announcements() -> Vec<Announcement> {
    ANNOUNCEMENTS.with(|queue| std::mem::take(&mut *queue.borrow_mut()))
}

/// Идентификатор узла, находящегося сейчас в фокусе.
pub fn focused_node() -> Option<NodeId> {
    FOCUSED.with(Cell::get)
}

/// Запоминает узел, получивший фокус (вызывает оконный бэкенд).
pub fn set_focused_node(node: Option<NodeId>) {
    FOCUSED.with(|cell| cell.set(node));
}

/// Программно переводит фокус на указанный узел.
///
/// Сама установка фокуса выполняется оконным бэкендом при следующей
/// перерисовке; здесь лишь фиксируется намерение.
pub fn focus(node_id: NodeId) {
    set_focused_node(Some(node_id));
}

/// Снимает фокус с активного узла.
pub fn blur() {
    set_focused_node(None);
}

/// Непрозрачная ссылка на активное (текущее) окно.
///
/// Позволяет управлять фокусом окна из прикладного кода. При выключенной
/// поддержке доступности [`active_window`] вернёт `None`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowHandle {
    token: crate::control::WindowToken,
}

impl WindowHandle {
    /// Токен окна, принятый в [`crate::control`].
    pub fn token(self) -> crate::control::WindowToken {
        self.token
    }

    /// Переводит фокус на узел этого окна.
    pub fn focus(self, node_id: NodeId) {
        focus(node_id);
    }
}

/// Активное окно, доступное для управления фокусом.
///
/// Возвращает `None`, если поддержка доступности выключена.
pub fn active_window() -> Option<WindowHandle> {
    if !is_enabled() {
        return None;
    }
    Some(WindowHandle {
        token: crate::control::active_window(),
    })
}

#[cfg(all(test, feature = "a11y"))]
mod tests {
    use super::*;

    #[test]
    fn announcements_are_queued_and_drained() {
        set_enabled(true);
        let _ = take_announcements();
        announce("saved", Politeness::Assertive);
        announce("updated", Politeness::Polite);
        let drained = take_announcements();
        assert_eq!(drained.len(), 2);
        assert_eq!(drained[0].message, "saved");
        assert_eq!(drained[0].politeness, Politeness::Assertive);
        assert_eq!(drained[1].politeness, Politeness::Polite);
        assert!(take_announcements().is_empty());
    }

    #[test]
    fn disabling_suppresses_announcements_and_window() {
        set_enabled(false);
        announce("ignored", Politeness::Polite);
        assert!(take_announcements().is_empty());
        assert_eq!(active_window(), None);
        set_enabled(true);
    }

    #[test]
    fn focus_roundtrip() {
        set_enabled(true);
        focus(NodeId(42));
        assert_eq!(focused_node(), Some(NodeId(42)));
        blur();
        assert_eq!(focused_node(), None);
    }

    #[test]
    fn politeness_parses_case_insensitively() {
        assert_eq!(Politeness::parse("POLITE"), Some(Politeness::Polite));
        assert_eq!(
            Politeness::parse(" assertive "),
            Some(Politeness::Assertive)
        );
        assert_eq!(Politeness::parse("off"), Some(Politeness::Off));
        assert_eq!(Politeness::parse("loud"), None);
    }
}
