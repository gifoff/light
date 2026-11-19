//! Состояние окна: `WindowState`, его поля и ввод/раскладка.
//!
//! Раскладка и перерисовка — в модуле [`render_loop`], обработка ввода (клик,
//! клавиатура, меню) — в модуле [`interaction`].

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use winit::window::{CursorIcon, Window};

use winit::keyboard::ModifiersState;

#[cfg(not(any(feature = "wgpu-backend", feature = "softbuffer-backend")))]
use crate::render::NullBackend;
use crate::render::RendererBackend;
#[cfg(all(feature = "wgpu-backend", not(feature = "softbuffer-backend")))]
use crate::render::WgpuBackend;
use light_vdom::component::{Component, WindowConfig};
use light_vdom::layout::{Layout, Point, Rect, TextMetrics};
use light_vdom::window_backend::{WindowToken, set_active_window, set_current_window_token};
use light_vdom::{ButtonType, Event as DomEvent, NodeKind, VElement};

use super::font::FontMetrics;
#[allow(unused_imports)]
use super::paint;
use super::render_loop::word_range;
#[allow(unused_imports)]
use super::text;

pub(super) struct ScaledMetrics<'a> {
    pub(super) inner: &'a FontMetrics,
    pub(super) scale: f32,
}

impl TextMetrics for ScaledMetrics<'_> {
    fn measure(&self, text: &str, font_size: f32) -> (f32, f32) {
        self.inner.measure(text, font_size * self.scale)
    }
}

pub(super) enum ClickAction {
    Click(&'static str),
    FocusField {
        key: &'static str,
        value: String,
        caret: usize,
    },
    #[cfg(feature = "elements-select")]
    ToggleSelect(&'static str),
}

/// Действие пункта контекстного меню поля ввода.
#[derive(Clone, Copy)]
pub(super) enum MenuAction {
    Cut,
    Copy,
    Paste,
    Delete,
    SelectAll,
}

/// Элемент стандартного меню по правой кнопке мыши.
#[derive(Clone)]
pub(super) struct MenuItem {
    pub(super) label: &'static str,
    pub(super) action: MenuAction,
    pub(super) enabled: bool,
}

/// Всплывающее меню поля ввода.
pub(crate) struct ContextMenu {
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) items: Vec<MenuItem>,
    pub(super) hover: Option<usize>,
}

/// Попадание по скроллбару: ключ прокручиваемого контейнера, ось и часть.
#[derive(Debug, Clone, Copy)]
pub(super) struct ScrollbarHit {
    pub(super) key: &'static str,
    pub(super) horizontal: bool,
    pub(super) on_thumb: bool,
}

/// Перетаскивание бегунка скроллбара: захват держится, пока нажата кнопка.
#[derive(Debug, Clone, Copy)]
pub(super) struct ScrollDrag {
    pub(super) key: &'static str,
    pub(super) horizontal: bool,
    /// Смещение точки захвата от начала бегунка (в пикселях).
    pub(super) grab: f32,
}

/// Состояние одного системного окна: компонент, поверхность, раскладка и ввод.
pub(super) struct WindowState {
    pub(super) token: WindowToken,
    pub(super) parent: Option<WindowToken>,
    pub(super) component: Box<dyn Component>,
    pub(super) font: Rc<FontMetrics>,
    pub(super) window: Arc<Window>,
    pub(super) surface: Box<dyn RendererBackend>,
    pub(super) config: WindowConfig,
    pub(super) title: String,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) scale: f32,
    pub(super) tree: Option<Arc<VElement>>,
    pub(super) layout: Option<Layout>,
    pub(super) full_redraw: bool,
    pub(super) pending_damage: Vec<Rect>,
    pub(super) focused: Option<String>,
    pub(super) editing: String,
    pub(super) caret: usize,
    pub(super) anchor: Option<usize>,
    pub(super) dragging: bool,
    pub(super) resizing: Option<&'static str>,
    pub(super) scroll_drag: Option<ScrollDrag>,
    pub(super) resized: HashMap<&'static str, (f32, f32)>,
    /// Смещения прокрутки прокручиваемых контейнеров по ключу узла.
    pub(super) scroll: HashMap<&'static str, Point>,
    /// Сообщения об ошибках валидации: ключ поля → текст (перерисовка поверх
    /// дерева компонента, как и `resized`).
    pub(super) validation: HashMap<String, String>,
    pub(super) modifiers: ModifiersState,
    pub(super) cursor: Point,
    pub(super) hover: Option<&'static str>,
    pub(super) hover_top: Option<&'static str>,
    pub(super) menu: Option<ContextMenu>,
    pub(super) clipboard: String,
    pub(super) undo: Vec<String>,
    pub(super) redo: Vec<String>,
    pub(super) cursor_icon: Option<CursorIcon>,
    pub(super) last_click: Option<(Instant, Point)>,
    #[cfg(feature = "elements-select")]
    pub(super) open_select: Option<&'static str>,
    #[cfg(feature = "elements-select")]
    pub(super) select_hover: Option<usize>,
    /// Адаптер AccessKit этого окна (если фича и поддержка включены).
    #[cfg(feature = "a11y-winit")]
    pub(super) adapter: Option<accesskit_winit::Adapter>,
    /// Ключ узла, получившего фокус через дерево доступности или `Tab`.
    #[cfg(feature = "a11y-winit")]
    pub(super) a11y_focus: Option<String>,
    /// Экземпляры кастомных элементов (Plugin API, Этап 5): ключ родительского
    /// узла → экземпляр. Заполняется при рендере из thread-local реестра
    /// [`crate::runtime`], используется при доставке событий через
    /// `on_event` (см. [`dispatch_custom_event`](Self::dispatch_custom_event)).
    pub(super) custom_instances: HashMap<String, Box<dyn light_vdom::plugins::CustomElement>>,
}

impl WindowState {
    pub(super) fn dispatch(&self, key: &str, event: DomEvent) -> bool {
        set_active_window(self.token);
        set_current_window_token(self.token);
        self.component.dispatch(key, event)
    }

    /// Доставляет событие ближайшему «родителю» кастомному элементу (Plugin API,
    /// Этап 5). Поднимается от `key` вверх по дереву, пока не найдётся ключ,
    /// заведённый в `custom_instances`; затем вызывает `on_event` на экземпляре
    /// и возвращает `true`, если плагин обработал событие (остановить доставку
    /// дальше) или запросил перерисовку через `schedule_redraw`.
    ///
    /// Возвращает `false`, если под `key` (и по пути вверх) нет кастомного
    /// элемента — тогда вызывающий код продолжает обычную доставку в
    /// [`Self::dispatch`].
    pub(super) fn dispatch_custom_event(&mut self, key: &str, event: &DomEvent) -> bool {
        // Пошагово поднимаемся по родительским ключам: строка вида "0.1.2" →
        // "0.1" → "0" → "" (корень). Для каждого проверяем, есть ли экземпляр.
        // Ключи в map — `String`, поэтому на каждом шаге создаём новый `String`.
        let mut cur: String = key.to_owned();
        loop {
            if self.custom_instances.contains_key(&cur) {
                // Забираем экземпляр из карты, чтобы `on_event` мог мутировать
                // его без конфликта заимствований с самой картой.
                let mut inst = self
                    .custom_instances
                    .remove(&cur)
                    .expect("just checked contains_key");
                // Флаг «хочу перерисовку»: `schedule_redraw` внутри `on_event`
                // ставит его через `Cell::set` (работает через `&dyn Fn()`).
                let want = std::cell::Cell::new(false);
                let flag = &want;
                let redraw: &dyn Fn() = &|| {
                    flag.set(true);
                };
                let mut ctx = light_vdom::plugins::EventCtx { redraw };
                inst.on_event(event, &mut ctx);
                // Возвращаем экземпляр на место.
                self.custom_instances.insert(cur.clone(), inst);
                // Если плагин поставил флаг — перерисовываемся (актуализируем
                // devtools-снимок + запрашиваем новый кадр).
                if want.get() {
                    self.invalidate();
                }
                // В текущем Этапе 5 любой обработанный кастомный элемент
                // останавливает дальнейшую доставку (по ТЗ: «если плагин
                // обработал/остановил propagation — не доставлять дальше»).
                return true;
            }
            // Поднимаемся на уровень выше: "0.1.2" → "0.1". Если корень — стоп.
            match cur.rsplit_once('.') {
                Some((parent, _)) => cur = parent.to_owned(),
                None => return false,
            }
        }
    }

    /// Отключён ли узел с ключом `key` в текущем дереве.
    ///
    /// Учитывает как встроенный флаг `disabled`, так и `aria-disabled` —
    /// через единую проверку [`crate::a11y::is_disabled`].
    pub(super) fn node_disabled(&self, key: &str) -> bool {
        self.tree
            .as_ref()
            .and_then(|tree| tree.get(key))
            .is_some_and(crate::a11y::is_disabled)
    }

    /// Доставляет событие компоненту, перехватывая отправку формы.
    ///
    /// Клик по кнопке `type="submit"` внутри `role="form"` сначала запускает
    /// валидацию: при ошибках событие поглощается (фокус — на первом
    /// невалидном поле), при успехе передаётся компоненту как обычно. Ввод в
    /// поле или переключение контрола сбрасывает ошибку этого поля.
    pub(super) fn dispatch_checked(&mut self, key: &str, event: DomEvent) -> bool {
        if self.node_disabled(key) {
            return true;
        }
        match &event {
            DomEvent::Click => {
                if self.handle_submit(key) {
                    return true;
                }
                self.clear_field_error(key);
            }
            DomEvent::TextChange(_) => self.clear_field_error(key),
            _ => {}
        }
        self.dispatch(key, event)
    }

    /// Сбрасывает ошибку валидации поля `key`, если она была.
    fn clear_field_error(&mut self, key: &str) {
        if self.validation.remove(key).is_some() {
            self.invalidate();
        }
    }

    /// Проверяет форму при нажатии кнопки отправки.
    ///
    /// Возвращает `true`, если форма невалидна и событие нужно поглотить.
    fn handle_submit(&mut self, key: &str) -> bool {
        let Some(tree) = self.tree.clone() else {
            return false;
        };
        let is_submit = tree.get(key).is_some_and(|node| {
            matches!(
                node.kind,
                NodeKind::Button {
                    disabled: false,
                    button_type: ButtonType::Submit,
                    ..
                }
            )
        });
        if !is_submit {
            return false;
        }
        let Some(form) = light_vdom::validation::form_ancestor(&tree, key) else {
            return false;
        };
        let form_key = form.key;
        let result = light_vdom::validation::validate_form(form, self.component.as_ref(), form_key);
        self.validation.clear();
        if result.is_valid() {
            self.invalidate();
            return false;
        }
        for error in &result.errors {
            self.validation
                .insert(error.key.clone(), error.message.clone());
        }
        if let Some(first) = result.first() {
            let field = first.key.clone();
            let message = first.message.clone();
            self.focused = Some(field.clone());
            #[cfg(feature = "a11y-winit")]
            {
                self.a11y_focus = Some(field);
            }
            #[cfg(feature = "a11y-live")]
            crate::a11y::announce(message, crate::a11y::Politeness::Assertive);
            #[cfg(not(feature = "a11y-live"))]
            let _ = message;
        }
        self.invalidate();
        true
    }

    /// Доставляет событие узлам под курсором от глубокого к корню.
    ///
    /// Если узел — кастомный элемент плагина, сначала пробует доставить
    /// событие ближайшему родителю кастомному (Этап 5 — on_event): если плагин
    /// обработал, доставка дальше не идёт. Иначе — обычная доставка в
    /// [`Self::dispatch`].
    pub(super) fn deliver(&mut self, event: DomEvent) -> bool {
        let Some(layout) = self.layout.as_ref() else {
            return false;
        };
        // Снимок ключей до входа в цикл: `dispatch_custom_event` заимствует
        // `self` mutably, поэтому `layout` (immutable borrow) должен завершить
        // своё использование перед первым вызовом.
        let keys: Vec<&'static str> = layout
            .hit_stack(self.cursor)
            .iter()
            .map(|n| n.key)
            .collect();
        for key in keys {
            if self.node_disabled(key) {
                return true;
            }
            // Кастомный элемент: пробуем on_event у ближайшего родителя.
            // Если плагин обработал — stop propagation (не ходить дальше).
            if self.dispatch_custom_event(key, &event) {
                return true;
            }
            if self.dispatch(key, event.clone()) {
                return true;
            }
        }
        false
    }

    /// Доставляет `@mousemove` узлам под курсором с координатами относительно узла.
    pub(super) fn deliver_mousemove(&self) -> bool {
        let Some(layout) = self.layout.as_ref() else {
            return false;
        };
        for node in layout.hit_stack(self.cursor) {
            let event = DomEvent::Mousemove {
                x: self.cursor.x - node.rect.x,
                y: self.cursor.y - node.rect.y,
            };
            if self.dispatch(node.key, event) {
                return true;
            }
        }
        false
    }

    pub(super) fn click_action(&self) -> Option<ClickAction> {
        let layout = self.layout.as_ref()?;
        let node = layout.hit_interactive(self.cursor)?;
        if self.node_disabled(node.key) {
            return None;
        }
        match &node.kind {
            NodeKind::Button { .. } => Some(ClickAction::Click(node.key)),
            #[cfg(feature = "elements-checkbox")]
            NodeKind::Checkbox { .. } => Some(ClickAction::Click(node.key)),
            #[cfg(feature = "elements-radio")]
            NodeKind::Radio { .. } => Some(ClickAction::Click(node.key)),
            NodeKind::TextField { value, .. } => Some(ClickAction::FocusField {
                key: node.key,
                value: value.clone(),
                caret: self.caret_from_pointer(node, self.cursor),
            }),
            #[cfg(feature = "elements-select")]
            NodeKind::Select { .. } => Some(ClickAction::ToggleSelect(node.key)),
            // Кастомный элемент плагина: клик доставляется как обычный
            // Click, чтобы @click-обработчик (и on_event плагина) сработали.
            NodeKind::Custom(_) => Some(ClickAction::Click(node.key)),
            _ => None,
        }
    }

    /// Позиция каретки (в символах) по точке клика внутри поля.
    ///
    /// Учитывает многострочную раскладку и горизонтальную прокрутку.
    pub(super) fn caret_from_pointer(
        &self,
        node: &light_vdom::layout::PlacedNode,
        point: Point,
    ) -> usize {
        let NodeKind::TextField {
            value, multiline, ..
        } = &node.kind
        else {
            return 0;
        };
        let rect = node.rect;
        let scale = self.scale;
        let inset = node.style.inset().scaled(scale);
        let pad_x = light_vdom::layout::FIELD_PAD_X * scale;
        let size = node
            .style
            .font_size
            .unwrap_or(light_vdom::layout::FONT_SIZE)
            * scale;
        let font = self.font.as_ref();
        let text_left = rect.x + inset.left + pad_x;
        let max_w = (rect.w - inset.horizontal() - 2.0 * pad_x).max(1.0);
        let lines = text::layout(font, value, size, max_w, *multiline);
        let len = value.chars().count();

        let (line, scroll_x) = if *multiline {
            let row_h = text::row_height(font, size);
            let content_top = rect.y + inset.top;
            let content_h = (rect.h - inset.vertical()).max(0.0);
            let visible = (content_h / row_h).floor().max(1.0);
            let caret_line = text::line_of(&lines, self.caret.min(len));
            let scroll_y = ((caret_line as f32 + 1.0 - visible).max(0.0)) * row_h;
            let local = point.y - content_top + scroll_y;
            let index = (local / row_h).floor().max(0.0) as usize;
            (lines[index.min(lines.len() - 1)].clone(), 0.0)
        } else {
            let caret_prefix = text::prefix_width(font, value, size, self.caret.min(len));
            let scroll_x = (caret_prefix - max_w).max(0.0);
            (lines[0].clone(), scroll_x)
        };

        let target = point.x - text_left + scroll_x;
        let count = line.text.chars().count();
        for i in 0..count {
            let left = text::prefix_width(font, &line.text, size, i);
            let right = text::prefix_width(font, &line.text, size, i + 1);
            if target < (left + right) / 2.0 {
                return line.start + i;
            }
        }
        line.start + count
    }

    /// Поле ввода под курсором: его ключ, значение и позицию каретки по клику.
    fn field_at_cursor(&self) -> Option<(&'static str, String, usize)> {
        let node = self.layout.as_ref()?.hit_interactive(self.cursor)?;
        if self.node_disabled(node.key) {
            return None;
        }
        let NodeKind::TextField { value, .. } = &node.kind else {
            return None;
        };
        Some((
            node.key,
            value.clone(),
            self.caret_from_pointer(node, self.cursor),
        ))
    }

    /// Выделяет слово под курсором — обработка двойного клика по полю ввода,
    /// как в системных полях: фокус ставится на поле, а его слово выделяется.
    ///
    /// Возвращает `true`, если под курсором оказалось текстовое поле; тогда
    /// обычная обработка клика (сброс выделения) не нужна.
    pub(super) fn select_word_at_cursor(&mut self) -> bool {
        let Some((key, value, index)) = self.field_at_cursor() else {
            return false;
        };
        self.focused = Some(key.to_owned());
        self.editing = value;
        let (start, end) = word_range(&self.editing, index);
        self.anchor = Some(start);
        self.caret = end;
        self.dragging = true;
        self.invalidate();
        true
    }
}
