//! Перерисовка, раскладка и повреждение (damage): второй `impl WindowState`
//! и вспомогательные функции.

use std::collections::HashMap;

/// Граница ресайза окна (физические пиксели).
pub(super) const RESIZE_BORDER: f32 = 6.0;
/// Порог двойного клика.
pub(super) const DOUBLE_CLICK: std::time::Duration = std::time::Duration::from_millis(400);

use winit::event::{ElementState, MouseButton, MouseScrollDelta};

#[allow(unused_imports)]
use super::paint::Canvas;
#[allow(unused_imports)]
use super::state::ScaledMetrics;
use light_vdom::layout::{Layout, Point, Rect, coalesce_regions, damage_regions, snapshot_rects};
use light_vdom::{Event as DomEvent, NodeKind, VElement, diff};
use std::sync::Arc;

use super::keys::map_button;
#[allow(unused_imports)]
use super::paint;
use super::scrollbar::{prune_scroll_offsets, scrollbar_strip_damage};
use super::state::WindowState;
use std::time::Instant;

impl WindowState {
    /// Снимает фокус при потере активности окна.
    pub(super) fn on_unfocus(&mut self) {
        self.dragging = false;
        self.resizing = None;
        self.menu = None;
        if self.focused.take().is_some() {
            self.anchor = None;
            self.undo.clear();
            self.redo.clear();
            self.invalidate();
        }
    }

    /// Прокрутка контейнера под курсором колесом мыши или трекпадом.
    pub(super) fn on_wheel(&mut self, delta: MouseScrollDelta) {
        let step = 32.0 * self.scale;
        let (dx, dy) = match delta {
            MouseScrollDelta::LineDelta(x, y) => (x * step, y * step),
            MouseScrollDelta::PixelDelta(position) => (position.x as f32, position.y as f32),
        };
        let mut changed = false;
        if dy != 0.0
            && let Some((key, max)) = self
                .layout
                .as_ref()
                .and_then(|layout| layout.scroll_target(self.cursor, false))
                .map(|node| (node.key, node.max_scroll_y()))
        {
            let offset = self.scroll.entry(key).or_insert(Point::new(0.0, 0.0));
            let next = (offset.y - dy).clamp(0.0, max);
            changed |= next != offset.y;
            offset.y = next;
        }
        if dx != 0.0
            && let Some((key, max)) = self
                .layout
                .as_ref()
                .and_then(|layout| layout.scroll_target(self.cursor, true))
                .map(|node| (node.key, node.max_scroll_x()))
        {
            let offset = self.scroll.entry(key).or_insert(Point::new(0.0, 0.0));
            let next = (offset.x - dx).clamp(0.0, max);
            changed |= next != offset.x;
            offset.x = next;
        }
        if changed {
            self.invalidate();
        }
    }

    /// Обработка нажатий/отпусканий кнопок мыши.
    pub(super) fn on_mouse(&mut self, state: ElementState, button: MouseButton) {
        match state {
            ElementState::Pressed => {
                if MouseButton::Left == button && self.handle_scrollbar_press() {
                    return;
                }
                self.deliver(DomEvent::Mousedown(map_button(button)));
                match button {
                    MouseButton::Left => {
                        let now = Instant::now();
                        let double = self.last_click.is_some_and(|(last, point)| {
                            now.duration_since(last) < DOUBLE_CLICK
                                && (point.x - self.cursor.x).abs() < 4.0
                                && (point.y - self.cursor.y).abs() < 4.0
                        });
                        self.last_click = Some((now, self.cursor));
                        if double {
                            self.deliver(DomEvent::Dblclick);
                            if self.select_word_at_cursor() {
                                return;
                            }
                        }
                        self.on_click();
                    }
                    MouseButton::Right => self.on_secondary_press(),
                    _ => {}
                }
            }
            ElementState::Released => {
                self.deliver(DomEvent::Mouseup(map_button(button)));
                if let MouseButton::Left = button {
                    self.on_release();
                }
            }
        }
    }
}

impl WindowState {
    pub(super) fn redraw(&mut self) -> crate::Result<()> {
        let window = Arc::clone(&self.window);
        self.scale = window.scale_factor() as f32;
        let size = window.inner_size();
        if self.resize(size.width, size.height)? {
            self.full_redraw = true;
        }
        light_vdom::window_backend::set_maximized(window.is_maximized());
        #[cfg(feature = "a11y-winit")]
        if let Some(command) = light_vdom::window_backend::take_focus_command() {
            self.apply_focus_command(command);
        }

        // Клик/изменение состояния DevTools применяются в UI-потоке через
        // [`UserEvent::DevTools`] (см. `App::handle_devtools_command`); здесь
        // только гарантируем актуальный снимок для TCP-сервера (основное
        // обновление — в `invalidate` при каждом изменении состояния).
        #[cfg(feature = "devtools")]
        self.update_devtools_snapshot();
        // Сбрасываем записи предыдущего кадра: после рендера компонента в
        // thread-local реестре накопятся только те экземпляры, которые создались
        // в этом кадре (Plugin API, Этап 5 — доставка on_event).
        light_vdom::runtime::drain_custom_instances();
        let mut tree = self.component.render();
        apply_overrides(&mut tree, &self.resized);
        apply_validation(&mut tree, &self.validation);
        self.update_title(&tree);
        let metrics = ScaledMetrics {
            inner: self.font.as_ref(),
            scale: self.scale,
        };
        let mut layout = Layout::build(&tree, (self.width as f32, self.height as f32), &metrics);
        layout.apply_scroll_offsets(&self.scroll);
        // Контейнеры, у которых больше нет прокрутки, сбрасываем: иначе при
        // следующем появлении большого контента он откроется уже прокрученным
        // в пустоту.
        prune_scroll_offsets(&mut self.scroll, &layout);
        let tree = Arc::new(tree);
        // Забираем свежие экземпляры кастомных элементов, созданных в этом
        // кадре, и передаём их в окно: теперь события можно доставлять им
        // через on_event.
        self.custom_instances = light_vdom::runtime::drain_custom_instances();
        let full = self.full_redraw;
        let mut damage = if full {
            vec![Rect::new(0.0, 0.0, self.width as f32, self.height as f32)]
        } else {
            match (self.tree.as_ref(), self.layout.as_ref()) {
                (Some(old_tree), Some(old_layout)) => {
                    let patch = diff(old_tree, &tree);
                    let mut damaged =
                        damage_regions(&snapshot_rects(old_layout), &layout, &patch.changed);
                    // Старые/новые полосы прокрутки: закрашиваем область, где
                    // раньше была полоса, чтобы не осталась «фантомная» после
                    // исчезновения прокрутки.
                    damaged.extend(scrollbar_strip_damage(old_layout, &layout, self.scale));
                    damaged
                }
                _ => vec![Rect::new(0.0, 0.0, self.width as f32, self.height as f32)],
            }
        };
        if full {
            self.pending_damage.clear();
        } else {
            damage.append(&mut self.pending_damage);
        }
        let damage = coalesce_regions(damage);
        light_vdom::dom::set_tree(self.token, Arc::clone(&tree));
        self.tree = Some(tree);
        self.layout = Some(layout);
        self.full_redraw = false;

        self.sync_focus_after_redraw();
        self.sync_select_after_redraw();
        #[cfg(feature = "a11y-winit")]
        {
            self.reconcile_a11y_focus();
            light_vdom::window_backend::set_focused_key(self.a11y_focus.clone());
            self.push_accessibility();
        }
        if damage.is_empty() {
            return Ok(());
        }
        self.paint(&damage);
        self.surface.present()?;
        Ok(())
    }

    fn update_title(&mut self, tree: &VElement) {
        let Some(title) = tree.titlebar_text() else {
            return;
        };
        if title != self.title {
            self.title = title;
            self.window.set_title(&self.title);
        }
    }

    fn sync_focus_after_redraw(&mut self) {
        let Some(key) = self.focused.clone() else {
            return;
        };
        let field = self.layout.as_ref().and_then(|layout| {
            layout
                .nodes()
                .iter()
                .find(|node| node.key == key)
                .and_then(|node| match &node.kind {
                    NodeKind::TextField {
                        value,
                        disabled: false,
                        ..
                    } => Some(value.clone()),
                    _ => None,
                })
        });
        match field {
            Some(value) => {
                let changed_externally = value != self.editing;
                let len = value.chars().count();
                self.editing = value.clone();
                self.caret = self.caret.min(len);
                self.anchor = self.anchor.map(|anchor| anchor.min(len));
                if changed_externally {
                    self.undo.clear();
                    self.redo.clear();
                }
            }
            None => {
                self.focused = None;
                self.editing.clear();
                self.caret = 0;
                self.anchor = None;
            }
        }
    }

    /// Сбрасывает состояние раскрытого списка, если его узел исчез из дерева.
    fn sync_select_after_redraw(&mut self) {
        #[cfg(feature = "elements-select")]
        {
            let Some(key) = self.open_select else {
                return;
            };
            let alive = self.layout.as_ref().is_some_and(|layout| {
                matches!(
                    layout.node(key).map(|node| &node.kind),
                    Some(NodeKind::Select {
                        disabled: false,
                        ..
                    })
                )
            });
            if !alive {
                self.open_select = None;
                self.select_hover = None;
            }
        }
    }

    fn resize(&mut self, width: u32, height: u32) -> crate::Result<bool> {
        if width == self.width && height == self.height {
            return Ok(false);
        }
        self.width = width;
        self.height = height;
        if width > 0 && height > 0 {
            self.surface.resize(width, height)?;
        }
        Ok(true)
    }

    fn paint(&mut self, damage: &[Rect]) {
        let Some(layout) = self.layout.as_ref() else {
            return;
        };
        let focused = self.focused.as_deref();
        #[cfg(feature = "a11y-winit")]
        let focus_ring = self.a11y_focus.as_deref();
        #[cfg(not(feature = "a11y-winit"))]
        let focus_ring: Option<&str> = None;
        let hover = self.hover;
        let selection = self.selection();
        #[cfg(feature = "elements-select")]
        let open_select = self.open_select;
        #[cfg(not(feature = "elements-select"))]
        let open_select: Option<&'static str> = None;
        #[cfg(feature = "elements-select")]
        let select_hover = self.select_hover;
        #[cfg(not(feature = "elements-select"))]
        let select_hover: Option<usize> = None;
        let background = self.config.background;
        let scale = self.scale;
        let menu = self.menu.as_ref();
        let (width, height) = self.surface.size();
        for region in damage {
            let mut canvas = Canvas::new(self.surface.frame_mut(), width, height, *region);
            paint::render(
                layout,
                &mut canvas,
                self.font.as_ref(),
                scale,
                paint::Interaction {
                    focused,
                    focus_ring,
                    caret_pos: self.caret,
                    selection,
                    hover,
                    open_select,
                    select_hover,
                },
                background,
            );
            if let Some(menu) = menu {
                paint::draw_context_menu(&mut canvas, menu, self.font.as_ref(), scale);
            }
        }
    }

    /// Выделенный диапазон (начало ≤ конец), если выделение не пустое.
    pub(super) fn selection(&self) -> Option<(usize, usize)> {
        self.focused.as_ref()?;
        let anchor = self.anchor?;
        if anchor == self.caret {
            return None;
        }
        Some((anchor.min(self.caret), anchor.max(self.caret)))
    }

    /// Удаляет выделение из буфера и возвращает изменённый текст.
    ///
    /// Возвращает `true`, если выделение было и текст изменился; каретка
    /// ставится в начало удалённого диапазона.
    pub(super) fn delete_selection(&mut self) -> bool {
        let Some((start, end)) = self.selection() else {
            return false;
        };
        let mut chars: Vec<char> = self.editing.chars().collect();
        chars.drain(start..end);
        self.editing = chars.into_iter().collect();
        self.caret = start;
        self.anchor = None;
        true
    }
}

/// Применяет сохранённые размеры полей (`resize`) к дереву перед раскладкой.
pub(super) fn apply_overrides(tree: &mut VElement, resized: &HashMap<&'static str, (f32, f32)>) {
    for (&key, &(width, height)) in resized {
        if let Some(node) = tree.get_mut(key) {
            node.style.width = light_vdom::Size::Px(width);
            node.style.height = light_vdom::Size::Px(height);
        }
    }
}

/// Помечает невалидные поля красной рамкой и добавляет текст ошибки для
/// диктора (`aria-invalid`/`aria-description`) поверх дерева компонента.
pub(super) fn apply_validation(tree: &mut VElement, validation: &HashMap<String, String>) {
    for (key, message) in validation {
        let Some(node) = tree.get_mut(key) else {
            continue;
        };
        node.style.invalid = true;
        upsert_attr(node, "invalid", "true".to_owned());
        upsert_attr(node, "aria-description", message.clone());
    }
}

/// Записывает атрибут, заменяя прежнее значение с тем же именем.
pub(super) fn upsert_attr(node: &mut VElement, name: &'static str, value: String) {
    if let Some(entry) = node.attrs.iter_mut().find(|(key, _)| *key == name) {
        entry.1 = value;
    } else {
        node.attrs.push((name, value));
    }
}

/// Границы слова (в символах) вокруг позиции `index` — для выделения двойным
/// кликом. Слово — непрерывная последовательность букв, цифр и `_`. Если под
/// позицией стоит несловесный символ (пробел, знак пунктуации), выделяется
/// только он; позиция сразу после слова «прилипает» к этому слову.
pub(crate) fn word_range(text: &str, index: usize) -> (usize, usize) {
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    if len == 0 {
        return (0, 0);
    }
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    let probe = if index < len && is_word(chars[index]) {
        index
    } else if index > 0 && is_word(chars[index - 1]) {
        index - 1
    } else {
        index.min(len - 1)
    };
    if !is_word(chars[probe]) {
        return (probe, probe + 1);
    }
    let mut start = probe;
    while start > 0 && is_word(chars[start - 1]) {
        start -= 1;
    }
    let mut end = probe + 1;
    while end < len && is_word(chars[end]) {
        end += 1;
    }
    (start, end)
}
