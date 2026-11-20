//! Канал команд управления окном.
//!
//! Шаблоны превращают `@click="close()"` в вызов `self.close()`. Методы
//! управления окном объявлены в трейте [`crate::component::Component`] с
//! реализацией по умолчанию, которая отправляет команду в этот канал.
//! Оконный бэкенд регистрирует получателя команд ([`set_command_sender`]).
//!
//! **Этап 3 (Strangler Fig):** вся логика перенесена в
//! [`light_vdom::window_backend`]; этот модуль — тонкий реэкспорт для
//! обратной совместимости с 0.19.x.

pub use light_vdom::window_backend::{
    FocusCommand, WindowCommand, WindowToken, active_window, blur, current_window_token, focus,
    focused_key, is_maximized, request, set_active_window, set_command_sender,
    set_current_window_token, set_focused_key, set_maximized, take_command, take_focus_command,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focus_request_round_trips() {
        set_focused_key(None);
        assert_eq!(focused_key(), None);
        assert_eq!(take_focus_command(), None);

        focus("field");
        assert_eq!(
            take_focus_command(),
            Some(FocusCommand::Focus("field".into()))
        );
        assert_eq!(take_focus_command(), None, "запрос потребляется один раз");

        blur();
        assert_eq!(take_focus_command(), Some(FocusCommand::Blur));

        set_focused_key(Some("field".into()));
        assert_eq!(focused_key().as_deref(), Some("field"));
        set_focused_key(None);
    }
}
