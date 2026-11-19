//! Открытие дочерних окон и применение команд управления окном.

use light_vdom::window_backend::{WindowCommand, WindowToken};
use winit::event_loop::ActiveEventLoop;

use super::app::App;

impl App {
    pub(super) fn open_modal(&mut self, event_loop: &ActiveEventLoop, token: WindowToken) {
        if self.is_blocked(token) {
            return;
        }
        let Some(id) = self.tokens.get(&token).copied() else {
            return;
        };
        let Some(component) = self
            .windows
            .get(&id)
            .and_then(|state| state.component.modal())
        else {
            return;
        };
        if let Err(error) = self.create_window(event_loop, component, Some(token)) {
            self.error = Some(error);
            event_loop.exit();
        }
    }

    /// Открывает ещё одно **главное** окно (без родителя) из компонента окна
    /// `token`, если тот предоставляет [`Component::window_factory`].
    pub(super) fn open_window(&mut self, event_loop: &ActiveEventLoop, token: WindowToken) {
        let Some(id) = self.tokens.get(&token).copied() else {
            return;
        };
        let Some(component) = self
            .windows
            .get(&id)
            .and_then(|state| state.component.window_factory())
        else {
            return;
        };
        if let Err(error) = self.create_window(event_loop, component, None) {
            self.error = Some(error);
            event_loop.exit();
        }
    }

    pub(super) fn apply_command(&self, token: WindowToken, command: WindowCommand) {
        let Some(id) = self.tokens.get(&token).copied() else {
            return;
        };
        let Some(state) = self.windows.get(&id) else {
            return;
        };
        let window = &state.window;
        match command {
            WindowCommand::Close
            | WindowCommand::OpenModal
            | WindowCommand::OpenWindow
            | WindowCommand::Open(_)
            | WindowCommand::CloseWindow(_)
            | WindowCommand::ShowWindow(_)
            | WindowCommand::HideWindow(_) => {}
            WindowCommand::Minimize => window.set_minimized(true),
            WindowCommand::Maximize => window.set_maximized(true),
            WindowCommand::Restore => window.set_maximized(false),
            WindowCommand::ToggleMaximize => window.set_maximized(!window.is_maximized()),
            WindowCommand::Show => window.set_visible(true),
            WindowCommand::Hide => window.set_visible(false),
        }
    }
}
