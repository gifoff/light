//! Обработка ввода `WindowState`: клик, клавиатура, меню, контекстное меню,
//! выделение и редактирование текста. Второй блок `impl WindowState`.

use winit::window::ResizeDirection;

use light_vdom::layout::{FIELD_MIN_H, Point};
use light_vdom::{Cursor, Event as DomEvent, NodeKind, Resize};

use super::cursor::{cursor_icon, resolve_cursor};
#[allow(unused_imports)]
use super::paint;
use super::render_loop::RESIZE_BORDER;
use super::state::{ClickAction, ContextMenu, MenuAction, MenuItem, WindowState};
#[allow(unused_imports)]
use super::text;

impl WindowState {
    pub(super) fn on_click(&mut self) {
        if self.menu.is_some() {
            self.on_menu_click();
            return;
        }
        if self.handle_open_select() {
            return;
        }
        if self.config.resizable
            && let Some(direction) = self.resize_direction()
        {
            let _ = self.window.drag_resize_window(direction);
            return;
        }
        match self.click_action() {
            Some(ClickAction::Click(key)) => {
                // Кастомный элемент плагина: сначала пробуем доставить событие
                // ближайшему родителю кастомному (Этап 5 — on_event). Если
                // плагин обработал — доставка дальше не нужна.
                if self.dispatch_custom_event(key, &DomEvent::Click) {
                    return;
                }
                self.dispatch_checked(key, DomEvent::Click);
                // После клика гарантированно перерисовать кадр **синхронно**
                // (в UI-потоке, где `&mut WindowState` доступен). `@click`-
                // хендлер виджета (например `show_toast`) меняет состояние
                // менеджерa в UI-потоке, но без этого вызова нет триггера
                // `request_redraw()`: когда цикл событий спит (нет других
                // событий ввода), тост/состояние оставались бы неперерисованными
                // до следующего события. `Invalidate` через прокси здесь
                // недостаточен: он лишь ставит флаг, но не будит цикл
                // (см. `events.rs` — ветка `UserEvent::Command` не вызывает
                // `request_redraw`).
                self.invalidate();
            }
            Some(ClickAction::FocusField { key, value, caret }) => {
                self.focused = Some(key.to_owned());
                self.editing = value;
                self.caret = caret;
                self.anchor = Some(caret);
                self.dragging = true;
                self.undo.clear();
                self.redo.clear();
                self.invalidate();
            }
            #[cfg(feature = "elements-select")]
            Some(ClickAction::ToggleSelect(key)) => {
                self.open_select = Some(key);
                self.select_hover = None;
                self.invalidate();
            }
            None => {
                if self.deliver(DomEvent::Click) {
                    return;
                }
                if self.is_draggable_titlebar() {
                    let _ = self.window.drag_window();
                    return;
                }
                if self.focused.take().is_some() {
                    self.invalidate();
                }
            }
        }
    }

    /// Обрабатывает клик при раскрытом выпадающем списке.
    ///
    /// Возвращает `true`, если клик поглощён (выбор пункта, закрытие списка);
    /// `false`, если список закрыт и клик следует обработать обычным путём.
    fn handle_open_select(&mut self) -> bool {
        #[cfg(feature = "elements-select")]
        {
            self.handle_open_select_impl()
        }
        #[cfg(not(feature = "elements-select"))]
        {
            false
        }
    }

    #[cfg(feature = "elements-select")]
    fn handle_open_select_impl(&mut self) -> bool {
        let Some(key) = self.open_select else {
            return false;
        };
        if let Some(index) = self.select_option_at(key) {
            let value = self
                .layout
                .as_ref()
                .and_then(|layout| layout.node(key))
                .and_then(|node| match &node.kind {
                    NodeKind::Select { options, .. } => options.get(index).map(|o| o.value.clone()),
                    _ => None,
                });
            self.open_select = None;
            self.select_hover = None;
            if let Some(value) = value {
                self.dispatch_checked(key, DomEvent::TextChange(value));
            }
            self.invalidate();
            return true;
        }
        let on_select = self
            .layout
            .as_ref()
            .and_then(|layout| layout.hit_interactive(self.cursor))
            .is_some_and(|node| node.key == key);
        self.open_select = None;
        self.select_hover = None;
        self.invalidate();
        on_select
    }

    /// Индекс пункта раскрытого списка под точкой.
    #[cfg(feature = "elements-select")]
    fn select_option_at(&self, key: &str) -> Option<usize> {
        let layout = self.layout.as_ref()?;
        let node = layout.node(key)?;
        let NodeKind::Select { options, .. } = &node.kind else {
            return None;
        };
        let popup = paint::select_popup(node.rect, options.len(), self.scale);
        if !popup.contains(self.cursor) {
            return None;
        }
        let row = paint::MENU_ROW_H * self.scale;
        if row <= 0.0 {
            return None;
        }
        let index = ((self.cursor.y - popup.y) / row) as usize;
        (index < options.len()).then_some(index)
    }

    /// Точка под курсором прижата к краю окна (для изменения размера).
    fn resize_direction(&self) -> Option<ResizeDirection> {
        let (x, y) = (self.cursor.x, self.cursor.y);
        let (w, h) = (self.width as f32, self.height as f32);
        let left = x <= RESIZE_BORDER;
        let right = x >= w - RESIZE_BORDER;
        let top = y <= RESIZE_BORDER;
        let bottom = y >= h - RESIZE_BORDER;
        match (left, right, top, bottom) {
            (true, _, true, _) => Some(ResizeDirection::NorthWest),
            (_, true, true, _) => Some(ResizeDirection::NorthEast),
            (true, _, _, true) => Some(ResizeDirection::SouthWest),
            (_, true, _, true) => Some(ResizeDirection::SouthEast),
            (true, _, _, _) => Some(ResizeDirection::West),
            (_, true, _, _) => Some(ResizeDirection::East),
            (_, _, true, _) => Some(ResizeDirection::North),
            (_, _, _, true) => Some(ResizeDirection::South),
            _ => None,
        }
    }

    /// Лежит ли точка внутри титульной полосы (`<titlebar>`).
    fn is_draggable_titlebar(&self) -> bool {
        self.layout
            .as_ref()
            .is_some_and(|layout| layout.hit_titlebar(self.cursor))
    }

    pub(super) fn on_release(&mut self) {
        if self.scroll_drag.take().is_some() {
            self.invalidate();
        }
        if self.resizing.take().is_some() {
            self.invalidate();
        }
        if self.dragging {
            self.dragging = false;
            self.invalidate();
        }
    }

    pub(super) fn on_cursor_moved(&mut self, x: f32, y: f32) {
        self.cursor = Point::new(x, y);

        if self.scroll_drag.is_some() {
            self.apply_scroll_drag();
            return;
        }

        if self.menu.is_some() {
            let hover = self.menu_hit(self.cursor);
            if self.menu.as_ref().is_some_and(|menu| menu.hover != hover) {
                if let Some(menu) = &mut self.menu {
                    menu.hover = hover;
                }
                self.invalidate();
            }
            return;
        }

        #[cfg(feature = "elements-select")]
        if let Some(key) = self.open_select {
            let hover = self.select_option_at(key);
            if hover != self.select_hover {
                self.select_hover = hover;
                self.invalidate();
            }
            return;
        }

        if let Some(key) = self.resizing {
            self.apply_resize(key);
            return;
        }

        if self.dragging && self.focused.is_some() {
            let caret = self
                .focused_node()
                .map(|node| self.caret_from_pointer(node, self.cursor));
            if let Some(caret) = caret
                && caret != self.caret
            {
                self.caret = caret;
                self.invalidate();
            }
            return;
        }

        let top = self.layout.as_ref().and_then(|layout| {
            // Текстовые листья не несут обработчиков, поэтому курсор
            // наведения относится к ближайшему элементу над ними (например,
            // к кнопке-контейнеру с текстом внутри).
            layout
                .hit_stack(self.cursor)
                .into_iter()
                .find(|node| !matches!(node.kind, NodeKind::Text { .. }))
                .map(|node| node.key)
        });
        if top != self.hover_top {
            if let Some(old) = self.hover_top {
                self.dispatch(old, DomEvent::Mouseleave);
            }
            if let Some(new) = top {
                self.dispatch(new, DomEvent::Mouseenter);
            }
            self.hover_top = top;
            // Перерисовка обязательна: обработчик `@mouseenter`/`@mouseleave`
            // мог изменить реактивное состояние (например, атомарный флаг,
            // проверяемый в `v-if`), и без `invalidate()` новое значение не
            // попадёт в следующий `render()`.
            self.invalidate();
        }
        if top.is_some() {
            self.deliver_mousemove();
        }

        let hovered = self
            .layout
            .as_ref()
            .and_then(|layout| layout.hit_interactive(self.cursor))
            .filter(|node| {
                matches!(
                    node.kind,
                    NodeKind::Button {
                        disabled: false,
                        ..
                    }
                )
            })
            .map(|node| node.key);
        if hovered != self.hover {
            let previous = self.hover;
            self.hover = hovered;
            self.invalidate_hover(previous, hovered);
        }
        self.update_cursor();
    }

    /// Устанавливает курсор окна в зависимости от того, где он находится:
    /// на области изменения размера — стрелки изменения размера, иначе —
    /// курсор ближайшего подходящего узла под указателем.
    fn update_cursor(&mut self) {
        let icon = if self.config.resizable
            && let Some(direction) = self.resize_direction()
        {
            direction.into()
        } else {
            let cursor = match (self.layout.as_ref(), self.tree.as_deref()) {
                (Some(layout), Some(tree)) => resolve_cursor(layout, tree, self.cursor),
                _ => Cursor::Default,
            };
            cursor_icon(cursor)
        };
        if self.cursor_icon != Some(icon) {
            self.cursor_icon = Some(icon);
            self.window.set_cursor(icon);
        }
    }

    /// Курсор покинул пределы окна.
    pub(super) fn on_cursor_left(&mut self) {
        if let Some(old) = self.hover_top.take() {
            self.dispatch(old, DomEvent::Mouseleave);
        }
    }

    pub(super) fn focused_node(&self) -> Option<&light_vdom::layout::PlacedNode> {
        let key = self.focused.as_deref()?;
        self.layout.as_ref()?.node(key)
    }

    /// Формирует меню у точки клика (с учётом заголовков пунктов).
    pub(super) fn open_menu_at(&mut self, point: Point) {
        let selection = self.selection();
        let has_selection = selection.is_some();
        let has_text = self.editing.chars().count() > 0;
        let items = vec![
            MenuItem {
                label: "Вырезать",
                action: MenuAction::Cut,
                enabled: has_selection,
            },
            MenuItem {
                label: "Копировать",
                action: MenuAction::Copy,
                enabled: has_selection,
            },
            MenuItem {
                label: "Вставить",
                action: MenuAction::Paste,
                enabled: !self.clipboard.is_empty(),
            },
            MenuItem {
                label: "Удалить",
                action: MenuAction::Delete,
                enabled: has_selection,
            },
            MenuItem {
                label: "Выделить всё",
                action: MenuAction::SelectAll,
                enabled: has_text,
            },
        ];
        let scale = self.scale;
        let w = paint::MENU_W * scale;
        let h = items.len() as f32 * paint::MENU_ROW_H * scale + 2.0 * paint::MENU_BORDER * scale;
        let x = point.x.clamp(2.0, (self.width as f32 - w - 2.0).max(2.0));
        let y = point.y.clamp(2.0, (self.height as f32 - h - 2.0).max(2.0));
        self.menu = Some(ContextMenu {
            x,
            y,
            items,
            hover: None,
        });
        self.full_redraw = true;
    }

    /// Индекс пункта меню под точкой (в пределах полного прямоугольника меню).
    fn menu_hit(&self, point: Point) -> Option<usize> {
        let menu = self.menu.as_ref()?;
        let scale = self.scale;
        let w = paint::MENU_W * scale;
        let h =
            menu.items.len() as f32 * paint::MENU_ROW_H * scale + 2.0 * paint::MENU_BORDER * scale;
        if point.x < menu.x || point.x > menu.x + w || point.y < menu.y || point.y > menu.y + h {
            return None;
        }
        let inner = paint::MENU_BORDER * scale;
        let row = paint::MENU_ROW_H * scale;
        let rel = point.y - menu.y - inner;
        if rel < 0.0 {
            return None;
        }
        let index = (rel / row) as usize;
        if index < menu.items.len() {
            Some(index)
        } else {
            None
        }
    }

    fn on_menu_click(&mut self) {
        let action = self
            .menu
            .as_ref()
            .and_then(|menu| self.menu_hit(self.cursor).map(|i| menu.items[i].action));
        self.menu = None;
        match action {
            Some(MenuAction::Cut) => self.cut_selection(),
            Some(MenuAction::Copy) => self.copy_selection(),
            Some(MenuAction::Paste) => self.paste_clipboard(),
            Some(MenuAction::Delete) => {
                let before = self.editing.clone();
                if self.delete_selection() {
                    self.snapshot_before_edit(&before);
                    self.commit_text();
                }
            }
            Some(MenuAction::SelectAll) => self.select_all(),
            None => {}
        }
        self.invalidate();
    }

    pub(super) fn copy_selection(&mut self) {
        if let Some((start, end)) = self.selection() {
            let text: String = self.editing.chars().skip(start).take(end - start).collect();
            self.clipboard = text;
        }
    }

    pub(super) fn cut_selection(&mut self) {
        let before = self.editing.clone();
        self.copy_selection();
        if self.delete_selection() {
            self.snapshot_before_edit(&before);
            self.commit_text();
        }
    }

    pub(super) fn paste_clipboard(&mut self) {
        let clip = self.clipboard.clone();
        if clip.is_empty() {
            return;
        }
        let before = self.editing.clone();
        self.delete_selection();
        self.snapshot_before_edit(&before);
        let mut chars: Vec<char> = self.editing.chars().collect();
        let at = self.caret.min(chars.len());
        for (offset, ch) in clip.chars().enumerate() {
            chars.insert(at + offset, ch);
        }
        self.caret = at + clip.chars().count();
        self.editing = chars.into_iter().collect();
        self.commit_text();
    }

    pub(super) fn snapshot_before_edit(&mut self, before: &str) {
        self.undo.push(before.to_owned());
        self.redo.clear();
    }

    pub(super) fn commit_text(&mut self) {
        if let Some(key) = self.focused.clone() {
            self.dispatch_checked(&key, DomEvent::TextChange(self.editing.clone()));
        }
        self.invalidate();
    }

    fn clamp_caret(&mut self) {
        let len = self.editing.chars().count();
        self.caret = self.caret.min(len);
        self.anchor = self.anchor.map(|anchor| anchor.min(len));
    }

    pub(super) fn undo(&mut self) {
        let Some(previous) = self.undo.pop() else {
            return;
        };
        self.redo.push(self.editing.clone());
        self.editing = previous;
        self.clamp_caret();
        self.commit_text();
    }

    pub(super) fn redo(&mut self) {
        let Some(next) = self.redo.pop() else {
            return;
        };
        self.undo.push(self.editing.clone());
        self.editing = next;
        self.clamp_caret();
        self.commit_text();
    }

    pub(super) fn select_all(&mut self) {
        self.anchor = Some(0);
        self.caret = self.editing.chars().count();
    }

    fn apply_resize(&mut self, key: &'static str) {
        let Some((rect, resize)) = self.layout.as_ref().and_then(|layout| {
            layout.node(key).and_then(|node| match &node.kind {
                NodeKind::TextField { resize, .. } => Some((node.rect, *resize)),
                _ => None,
            })
        }) else {
            return;
        };
        let min = FIELD_MIN_H * self.scale;
        let (mut w, mut h) = self.resized.get(key).copied().unwrap_or((rect.w, rect.h));
        if matches!(resize, Resize::Horizontal | Resize::Both) {
            w = (self.cursor.x - rect.x).max(min);
        }
        if matches!(resize, Resize::Vertical | Resize::Both) {
            h = (self.cursor.y - rect.y).max(min);
        }
        self.resized.insert(key, (w, h));
        self.invalidate();
    }
}
