//! Обработка клавиатуры `WindowState`: ввод текста, навигация каретки, undo/redo,
//! submit и команды фокуса. Третий блок `impl WindowState`.

use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{Key, NamedKey};

#[allow(unused_imports)]
use super::text;
use light_vdom::{Event as DomEvent, NodeKind};

use super::keys::{control_letter, key_info};
use super::state::WindowState;

impl WindowState {
    /// Отправляет форму по `Enter` из однострочного поля ввода.
    ///
    /// Срабатывает, только если фокус — на включённом однострочном `<text>`
    /// внутри `role="form"`, нет модификаторов (`Shift`/`Ctrl`/`Alt`) и в форме
    /// есть включённая кнопка отправки. Многострочные поля вставляют перевод
    /// строки, поэтому `Enter` в них форму не отправляет. Возвращает `true`,
    /// если нажатие обработано и не должно попадать в логику редактирования.
    fn try_submit_on_enter(&mut self, event: &KeyEvent) -> bool {
        if event.state != ElementState::Pressed
            || !matches!(&event.logical_key, Key::Named(NamedKey::Enter))
            || self.modifiers.shift_key()
            || self.modifiers.control_key()
            || self.modifiers.alt_key()
        {
            return false;
        }
        let Some(tree) = self.tree.clone() else {
            return false;
        };
        let Some(key) = self.focused.clone() else {
            return false;
        };
        let is_single_line = matches!(
            tree.get(&key).map(|node| &node.kind),
            Some(NodeKind::TextField {
                multiline: false,
                disabled: false,
                ..
            })
        );
        if !is_single_line {
            return false;
        }
        let Some(button) = light_vdom::validation::find_submit_button(&tree, &key) else {
            return false;
        };
        let button_key = button.key.to_owned();
        self.dispatch_checked(&button_key, DomEvent::Click);
        self.invalidate();
        true
    }

    pub(super) fn on_key(&mut self, event: &KeyEvent) {
        #[cfg(feature = "elements-select")]
        if self.open_select.is_some() && matches!(&event.logical_key, Key::Named(NamedKey::Escape))
        {
            self.open_select = None;
            self.select_hover = None;
            self.invalidate();
            return;
        }
        if self.try_submit_on_enter(event) {
            return;
        }
        let info = key_info(event, self.modifiers);
        let key = self.focused.clone().unwrap_or_default();
        match event.state {
            ElementState::Pressed => {
                #[cfg(feature = "a11y-winit")]
                {
                    let handled = self.dispatch(&key, DomEvent::Keydown(info.clone()));
                    if !handled && self.handle_builtin_key(event) {
                        return;
                    }
                }
                #[cfg(not(feature = "a11y-winit"))]
                self.dispatch(&key, DomEvent::Keydown(info));
            }
            ElementState::Released => {
                self.dispatch(&key, DomEvent::Keyup(info));
            }
        }
        if event.state != ElementState::Pressed {
            return;
        }
        let Some(focused) = self.focused.clone() else {
            return;
        };
        let extend = self.modifiers.shift_key();
        let before = self.editing.clone();
        let mut text_changed = false;
        let mut caret_changed = false;
        match &event.logical_key {
            Key::Named(NamedKey::Backspace) => {
                if self.delete_selection() {
                    text_changed = true;
                    caret_changed = true;
                } else if self.caret > 0 {
                    let mut chars: Vec<char> = self.editing.chars().collect();
                    chars.remove(self.caret - 1);
                    self.editing = chars.into_iter().collect();
                    self.caret -= 1;
                    text_changed = true;
                    caret_changed = true;
                }
            }
            Key::Named(NamedKey::Delete) => {
                if self.delete_selection() {
                    text_changed = true;
                    caret_changed = true;
                } else {
                    let len = self.editing.chars().count();
                    if self.caret < len {
                        let mut chars: Vec<char> = self.editing.chars().collect();
                        chars.remove(self.caret);
                        self.editing = chars.into_iter().collect();
                        text_changed = true;
                    }
                }
            }
            Key::Named(NamedKey::Enter) => {
                if self.multiline_focused() {
                    self.delete_selection();
                    let mut chars: Vec<char> = self.editing.chars().collect();
                    chars.insert(self.caret, '\n');
                    self.caret += 1;
                    self.anchor = None;
                    self.editing = chars.into_iter().collect();
                    text_changed = true;
                    caret_changed = true;
                }
            }
            Key::Named(NamedKey::ArrowLeft) => {
                self.move_caret(self.caret.saturating_sub(1), extend);
                caret_changed = true;
            }
            Key::Named(NamedKey::ArrowRight) => {
                self.move_caret(self.caret + 1, extend);
                caret_changed = true;
            }
            Key::Named(NamedKey::ArrowUp) => {
                self.move_caret_home(extend);
                self.move_caret_vertical(-1, extend);
                caret_changed = true;
            }
            Key::Named(NamedKey::ArrowDown) => {
                self.move_caret_vertical(1, extend);
                caret_changed = true;
            }
            Key::Named(NamedKey::Home) => {
                self.move_caret_home(extend);
                caret_changed = true;
            }
            Key::Named(NamedKey::End) => {
                self.move_caret_end(extend);
                caret_changed = true;
            }
            _ => {
                if self.modifiers.control_key() {
                    let mapped = control_letter(event);
                    match mapped.as_deref() {
                        Some("a") => {
                            self.select_all();
                            caret_changed = true;
                        }
                        Some("c") => self.copy_selection(),
                        Some("x") => self.cut_selection(),
                        Some("v") => self.paste_clipboard(),
                        Some("z") if self.modifiers.shift_key() => self.redo(),
                        Some("z") => self.undo(),
                        Some("y") => self.redo(),
                        _ => {}
                    }
                    if caret_changed {
                        self.invalidate();
                    }
                    return;
                }
                let Some(text) = event.text.as_deref() else {
                    return;
                };
                if text.is_empty() || text.chars().any(char::is_control) {
                    return;
                }
                self.delete_selection();
                let mut chars: Vec<char> = self.editing.chars().collect();
                let mut idx = self.caret;
                for c in text.chars() {
                    chars.insert(idx, c);
                    idx += 1;
                }
                self.caret = idx;
                self.anchor = None;
                self.editing = chars.into_iter().collect();
                text_changed = true;
                caret_changed = true;
            }
        }

        if text_changed {
            if self.editing != before {
                self.snapshot_before_edit(&before);
            }
            self.dispatch_checked(&focused, DomEvent::TextChange(self.editing.clone()));
        }
        // Перемещение каретки не меняет значение поля, поэтому реактивная
        // перерисовка не сработает — запрашиваем её вручную.
        if caret_changed && !text_changed {
            self.invalidate();
        }
    }

    pub(super) fn on_secondary_press(&mut self) {
        if let Some((key, value, caret)) = self
            .layout
            .as_ref()
            .and_then(|layout| layout.hit_interactive(self.cursor))
            .and_then(|node| {
                if self.node_disabled(node.key) {
                    return None;
                }
                match &node.kind {
                    NodeKind::TextField { value, .. } => Some((
                        node.key,
                        value.clone(),
                        self.caret_from_pointer(node, self.cursor),
                    )),
                    _ => None,
                }
            })
        {
            // Если поле уже в фокусе и текст выделен, ПКМ не сбрасывает
            // выделение — контекстное меню открывается поверх него.
            let keep_selection = self.focused.as_deref() == Some(key) && self.selection().is_some();
            if !keep_selection {
                self.focused = Some(key.to_owned());
                self.editing = value;
                self.caret = caret;
                self.anchor = None;
                self.dragging = false;
                self.undo.clear();
                self.redo.clear();
            }
            self.open_menu_at(self.cursor);
            self.invalidate();
        }
    }

    /// Перерисовывает только области старой и новой кнопки под курсором.
    pub(super) fn invalidate_hover(
        &mut self,
        previous: Option<&'static str>,
        current: Option<&'static str>,
    ) {
        let Some(layout) = self.layout.as_ref() else {
            self.invalidate();
            return;
        };
        for key in previous.into_iter().chain(current) {
            if let Some(node) = layout.node(key) {
                self.pending_damage.push(node.rect);
            }
        }
        self.window.request_redraw();
    }

    pub(super) fn invalidate(&mut self) {
        self.full_redraw = true;
        #[cfg(feature = "devtools")]
        self.update_devtools_snapshot();
        self.window.request_redraw();
    }

    /// Обновляет снимок DevTools (`LATEST_SNAPSHOT`) при изменении состояния.
    ///
    /// Вызывается из [`invalidate`] — то есть при каждом изменении реактивного
    /// свойства, а не только при перерисовке. Так TCP-инспектор сразу видит
    /// актуальный `state` (например `count`), не дожидаясь следующего кадра;
    /// иначе `inspect_state` возвращал бы устаревшее значение.
    #[cfg(feature = "devtools")]
    pub(super) fn update_devtools_snapshot(&self) {
        let snap = crate::devtools::capture_snapshot(self.token as u32, self.component.as_ref());
        if let Ok(json) = serde_json::to_string(&snap) {
            crate::devtools::update_latest_snapshot(json);
        }
    }

    /// Применяет программный запрос фокуса (`light_vdom::window_backend::focus`/`blur`).
    #[cfg(feature = "a11y-winit")]
    pub(super) fn apply_focus_command(
        &mut self,
        command: light_vdom::window_backend::FocusCommand,
    ) {
        match command {
            light_vdom::window_backend::FocusCommand::Focus(key) => self.set_a11y_focus(Some(key)),
            light_vdom::window_backend::FocusCommand::Blur => self.set_a11y_focus(None),
        }
        self.invalidate();
    }

    pub(super) fn multiline_focused(&self) -> bool {
        matches!(
            self.focused_node().map(|node| &node.kind),
            Some(NodeKind::TextField {
                multiline: true,
                ..
            })
        )
    }

    fn field_lines(&self) -> Option<Vec<text::Line>> {
        let node = self.focused_node()?;
        let NodeKind::TextField { multiline, .. } = &node.kind else {
            return None;
        };
        let scale = self.scale;
        let inset = node.style.inset().scaled(scale);
        let pad_x = light_vdom::layout::FIELD_PAD_X * scale;
        let size = node
            .style
            .font_size
            .unwrap_or(light_vdom::layout::FONT_SIZE)
            * scale;
        let max_w = (node.rect.w - inset.horizontal() - 2.0 * pad_x).max(1.0);
        Some(text::layout(
            self.font.as_ref(),
            &self.editing,
            size,
            max_w,
            *multiline,
        ))
    }

    fn move_caret(&mut self, to: usize, extend: bool) {
        let len = self.editing.chars().count();
        if extend {
            if self.anchor.is_none() {
                self.anchor = Some(self.caret);
            }
        } else {
            self.anchor = None;
        }
        self.caret = to.min(len);
    }

    fn move_caret_home(&mut self, extend: bool) {
        let index = self
            .field_lines()
            .map(|lines| text::line_of(&lines, self.caret.min(self.editing.chars().count())))
            .unwrap_or(0);
        let to = self
            .field_lines()
            .map(|lines| lines[index].start)
            .unwrap_or(0);
        self.move_caret(to, extend);
    }

    fn move_caret_end(&mut self, extend: bool) {
        let Some(lines) = self.field_lines() else {
            let len = self.editing.chars().count();
            self.move_caret(len, extend);
            return;
        };
        let index = text::line_of(&lines, self.caret.min(self.editing.chars().count()));
        let line = &lines[index];
        let to = line.start + line.text.chars().count();
        self.move_caret(to, extend);
    }

    fn move_caret_vertical(&mut self, delta: i32, extend: bool) {
        let Some(lines) = self.field_lines() else {
            return;
        };
        if lines.len() < 2 {
            return;
        }
        let caret = self.caret.min(self.editing.chars().count());
        let index = text::line_of(&lines, caret) as i32;
        let target = index + delta;
        if target < 0 || target >= lines.len() as i32 {
            return;
        }
        let current = &lines[index as usize];
        let column = caret.saturating_sub(current.start);
        let text = &lines[target as usize].text;
        let column = column.min(text.chars().count());
        let to = lines[target as usize].start + column;
        self.move_caret(to, extend);
    }
}
