//! Оконная обвязка поверх `winit` и сменного бэкенда рендеринга.
//!
//! Создаёт безрамочное окно (параметры берутся из [`Component::window_config`]),
//! рисует интерфейс программным растеризатором в RGBA-буфер, обрабатывает
//! ввод, перетаскивание за `<titlebar>` и команды управления окном
//! (`close()`, `minimize()`, `maximize()`, …).
//!
//! Бэкенд поддерживает несколько окон: главное окно и дочерние модальные
//! (`Component::modal`). Команда адресуется окну по [`WindowToken`], который
//! запоминается перед обработкой события.

mod font;
mod icon;
#[cfg(feature = "image-support")]
mod image;
pub(crate) mod paint;
mod text;

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
#[cfg(feature = "a11y-winit")]
use winit::event_loop::EventLoopProxy;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key, KeyCode, ModifiersState, NamedKey, PhysicalKey};
use winit::window::{CursorIcon, ResizeDirection, Window, WindowId};

use crate::Error;
use crate::component::{Component, WindowConfig};
use crate::control::{WindowCommand, WindowToken, set_active_window, set_command_sender};
use crate::layout::{
    FIELD_MIN_H, Layout, Point, Rect, TextMetrics, coalesce_regions, damage_regions, snapshot_rects,
};
use crate::reactive::set_repaint_requester;
#[cfg(not(any(feature = "wgpu-backend", feature = "softbuffer-backend")))]
use crate::render::NullBackend;
use crate::render::RendererBackend;
#[cfg(feature = "softbuffer-backend")]
use crate::render::SoftbufferBackend;
#[cfg(all(feature = "wgpu-backend", not(feature = "softbuffer-backend")))]
use crate::render::WgpuBackend;
use crate::vdom::{
    ButtonType, Cursor, Event as DomEvent, KeyInfo, MouseButton as DomButton, NodeKind, Resize,
    VElement, diff,
};

use font::FontMetrics;
use paint::Canvas;

/// Толщина зоны у края окна, тянуть за которую меняет размер.
const RESIZE_BORDER: f32 = 6.0;
/// Промежуток, в течение которого два нажатия считаются двойным кликом.
const DOUBLE_CLICK: Duration = Duration::from_millis(400);

/// Пользовательское событие цикла: команда окна с адресатом-токеном либо
/// событие AccessKit (запрос дерева, действие диктора, деактивация).
enum UserEvent {
    /// Команда управления окном с адресатом-токеном.
    Command {
        token: WindowToken,
        command: WindowCommand,
    },
    /// Событие от адаптера AccessKit.
    #[cfg(feature = "a11y-winit")]
    Access(accesskit_winit::Event),
}

#[cfg(feature = "a11y-winit")]
impl From<accesskit_winit::Event> for UserEvent {
    fn from(event: accesskit_winit::Event) -> Self {
        UserEvent::Access(event)
    }
}

/// Запускает цикл событий для компонента.
///
/// Если [`Component::window_config`] возвращает `None`, цикл событий не запускается
/// и функция сразу завершается успешно.
pub fn run(component: impl Component + 'static) -> crate::Result<()> {
    if component.window_config().is_none() {
        return Ok(());
    }
    let font = Rc::new(FontMetrics::load()?);
    let mut builder = EventLoop::<UserEvent>::with_user_event();
    let event_loop = builder.build()?;
    let proxy = event_loop.create_proxy();
    let sender = proxy.clone();
    set_command_sender(move |token, command| {
        let _ = sender.send_event(UserEvent::Command { token, command });
    });
    let mut app = App::new(Box::new(component), font);
    #[cfg(feature = "a11y-winit")]
    {
        app.proxy = Some(proxy);
    }
    event_loop.run_app(&mut app)?;
    match app.error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

struct ScaledMetrics<'a> {
    inner: &'a FontMetrics,
    scale: f32,
}

impl TextMetrics for ScaledMetrics<'_> {
    fn measure(&self, text: &str, font_size: f32) -> (f32, f32) {
        self.inner.measure(text, font_size * self.scale)
    }
}

enum ClickAction {
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
enum MenuAction {
    Cut,
    Copy,
    Paste,
    Delete,
    SelectAll,
}

/// Элемент стандартного меню по правой кнопке мыши.
#[derive(Clone)]
struct MenuItem {
    label: &'static str,
    action: MenuAction,
    enabled: bool,
}

/// Всплывающее меню поля ввода.
struct ContextMenu {
    x: f32,
    y: f32,
    items: Vec<MenuItem>,
    hover: Option<usize>,
}

/// Состояние одного системного окна: компонент, поверхность, раскладка и ввод.
struct WindowState {
    token: WindowToken,
    parent: Option<WindowToken>,
    component: Box<dyn Component>,
    font: Rc<FontMetrics>,
    window: Arc<Window>,
    surface: Box<dyn RendererBackend>,
    config: WindowConfig,
    title: String,
    width: u32,
    height: u32,
    scale: f32,
    tree: Option<Arc<VElement>>,
    layout: Option<Layout>,
    full_redraw: bool,
    pending_damage: Vec<Rect>,
    focused: Option<String>,
    editing: String,
    caret: usize,
    anchor: Option<usize>,
    dragging: bool,
    resizing: Option<&'static str>,
    resized: HashMap<&'static str, (f32, f32)>,
    /// Смещения прокрутки прокручиваемых контейнеров по ключу узла.
    scroll: HashMap<&'static str, Point>,
    /// Сообщения об ошибках валидации: ключ поля → текст (перерисовка поверх
    /// дерева компонента, как и `resized`).
    validation: HashMap<String, String>,
    modifiers: ModifiersState,
    cursor: Point,
    hover: Option<&'static str>,
    hover_top: Option<&'static str>,
    menu: Option<ContextMenu>,
    clipboard: String,
    undo: Vec<String>,
    redo: Vec<String>,
    cursor_icon: Option<CursorIcon>,
    last_click: Option<(Instant, Point)>,
    #[cfg(feature = "elements-select")]
    open_select: Option<&'static str>,
    #[cfg(feature = "elements-select")]
    select_hover: Option<usize>,
    /// Адаптер AccessKit этого окна (если фича и поддержка включены).
    #[cfg(feature = "a11y-winit")]
    adapter: Option<accesskit_winit::Adapter>,
    /// Ключ узла, получившего фокус через дерево доступности или `Tab`.
    #[cfg(feature = "a11y-winit")]
    a11y_focus: Option<String>,
}

impl WindowState {
    fn redraw(&mut self) -> crate::Result<()> {
        let window = Arc::clone(&self.window);
        self.scale = window.scale_factor() as f32;
        let size = window.inner_size();
        if self.resize(size.width, size.height)? {
            self.full_redraw = true;
        }
        crate::control::set_maximized(window.is_maximized());
        #[cfg(feature = "a11y-winit")]
        if let Some(command) = crate::control::take_focus_command() {
            self.apply_focus_command(command);
        }

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
        let tree = Arc::new(tree);
        let full = self.full_redraw;
        let mut damage = if full {
            vec![Rect::new(0.0, 0.0, self.width as f32, self.height as f32)]
        } else {
            match (self.tree.as_ref(), self.layout.as_ref()) {
                (Some(old_tree), Some(old_layout)) => {
                    let patch = diff(old_tree, &tree);
                    damage_regions(&snapshot_rects(old_layout), &layout, &patch.changed)
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
        crate::dom::set_tree(self.token, Arc::clone(&tree));
        self.tree = Some(tree);
        self.layout = Some(layout);
        self.full_redraw = false;

        self.sync_focus_after_redraw();
        self.sync_select_after_redraw();
        #[cfg(feature = "a11y-winit")]
        {
            self.reconcile_a11y_focus();
            crate::control::set_focused_key(self.a11y_focus.clone());
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
        let font = self.font.font();
        let scale = self.scale;
        let menu = self.menu.as_ref();
        let (width, height) = self.surface.size();
        for region in damage {
            let mut canvas = Canvas::new(self.surface.frame_mut(), width, height, *region);
            paint::render(
                layout,
                &mut canvas,
                font,
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
                paint::draw_context_menu(&mut canvas, menu, font, scale);
            }
        }
    }

    /// Выделенный диапазон (начало ≤ конец), если выделение не пустое.
    fn selection(&self) -> Option<(usize, usize)> {
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
    fn delete_selection(&mut self) -> bool {
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

    /// Отправляет событие компоненту окна, запоминая окно как активное.
    fn dispatch(&self, key: &str, event: DomEvent) -> bool {
        set_active_window(self.token);
        self.component.dispatch(key, event)
    }

    /// Отключён ли узел с ключом `key` в текущем дереве.
    ///
    /// Учитывает как встроенный флаг `disabled`, так и `aria-disabled` —
    /// через единую проверку [`crate::a11y::is_disabled`].
    fn node_disabled(&self, key: &str) -> bool {
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
    fn dispatch_checked(&mut self, key: &str, event: DomEvent) -> bool {
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
        let Some(form) = crate::validation::form_ancestor(&tree, key) else {
            return false;
        };
        let form_key = form.key;
        let result = crate::validation::validate_form(form, self.component.as_ref(), form_key);
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
    fn deliver(&self, event: DomEvent) -> bool {
        let Some(layout) = self.layout.as_ref() else {
            return false;
        };
        for node in layout.hit_stack(self.cursor) {
            if self.node_disabled(node.key) {
                return true;
            }
            if self.dispatch(node.key, event.clone()) {
                return true;
            }
        }
        false
    }

    /// Доставляет `@mousemove` узлам под курсором с координатами относительно узла.
    fn deliver_mousemove(&self) -> bool {
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

    fn click_action(&self) -> Option<ClickAction> {
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
            _ => None,
        }
    }

    /// Позиция каретки (в символах) по точке клика внутри поля.
    ///
    /// Учитывает многострочную раскладку и горизонтальную прокрутку.
    fn caret_from_pointer(&self, node: &crate::layout::PlacedNode, point: Point) -> usize {
        let NodeKind::TextField {
            value, multiline, ..
        } = &node.kind
        else {
            return 0;
        };
        let rect = node.rect;
        let scale = self.scale;
        let inset = node.style.inset().scaled(scale);
        let pad_x = crate::layout::FIELD_PAD_X * scale;
        let size = node.style.font_size.unwrap_or(crate::layout::FONT_SIZE) * scale;
        let font = self.font.font();
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
    fn select_word_at_cursor(&mut self) -> bool {
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

    fn on_click(&mut self) {
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
                self.dispatch_checked(key, DomEvent::Click);
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

    fn on_release(&mut self) {
        if self.resizing.take().is_some() {
            self.invalidate();
        }
        if self.dragging {
            self.dragging = false;
            self.invalidate();
        }
    }

    fn on_cursor_moved(&mut self, x: f32, y: f32) {
        self.cursor = Point::new(x, y);

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
    fn on_cursor_left(&mut self) {
        if let Some(old) = self.hover_top.take() {
            self.dispatch(old, DomEvent::Mouseleave);
        }
    }

    fn focused_node(&self) -> Option<&crate::layout::PlacedNode> {
        let key = self.focused.as_deref()?;
        self.layout.as_ref()?.node(key)
    }

    /// Формирует меню у точки клика (с учётом заголовков пунктов).
    fn open_menu_at(&mut self, point: Point) {
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

    fn copy_selection(&mut self) {
        if let Some((start, end)) = self.selection() {
            let text: String = self.editing.chars().skip(start).take(end - start).collect();
            self.clipboard = text;
        }
    }

    fn cut_selection(&mut self) {
        let before = self.editing.clone();
        self.copy_selection();
        if self.delete_selection() {
            self.snapshot_before_edit(&before);
            self.commit_text();
        }
    }

    fn paste_clipboard(&mut self) {
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

    fn snapshot_before_edit(&mut self, before: &str) {
        self.undo.push(before.to_owned());
        self.redo.clear();
    }

    fn commit_text(&mut self) {
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

    fn undo(&mut self) {
        let Some(previous) = self.undo.pop() else {
            return;
        };
        self.redo.push(self.editing.clone());
        self.editing = previous;
        self.clamp_caret();
        self.commit_text();
    }

    fn redo(&mut self) {
        let Some(next) = self.redo.pop() else {
            return;
        };
        self.undo.push(self.editing.clone());
        self.editing = next;
        self.clamp_caret();
        self.commit_text();
    }

    fn select_all(&mut self) {
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

    fn multiline_focused(&self) -> bool {
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
        let pad_x = crate::layout::FIELD_PAD_X * scale;
        let size = node.style.font_size.unwrap_or(crate::layout::FONT_SIZE) * scale;
        let max_w = (node.rect.w - inset.horizontal() - 2.0 * pad_x).max(1.0);
        Some(text::layout(
            self.font.font(),
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
        let Some(button) = crate::validation::find_submit_button(&tree, &key) else {
            return false;
        };
        let button_key = button.key.to_owned();
        self.dispatch_checked(&button_key, DomEvent::Click);
        self.invalidate();
        true
    }

    fn on_key(&mut self, event: &KeyEvent) {
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

    fn on_secondary_press(&mut self) {
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
    fn invalidate_hover(&mut self, previous: Option<&'static str>, current: Option<&'static str>) {
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

    fn invalidate(&mut self) {
        self.full_redraw = true;
        self.window.request_redraw();
    }

    /// Применяет программный запрос фокуса (`crate::control::focus`/`blur`).
    #[cfg(feature = "a11y-winit")]
    fn apply_focus_command(&mut self, command: crate::control::FocusCommand) {
        match command {
            crate::control::FocusCommand::Focus(key) => self.set_a11y_focus(Some(key)),
            crate::control::FocusCommand::Blur => self.set_a11y_focus(None),
        }
        self.invalidate();
    }
}

/// Интеграция с AccessKit: дерево доступности, действия диктора и клавиатура.
#[cfg(feature = "a11y-winit")]
impl WindowState {
    /// Отправляет текущее дерево доступности адаптеру (если тот активен).
    fn push_accessibility(&mut self) {
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
    fn handle_action_request(&mut self, request: &accesskit::ActionRequest) {
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
    fn set_a11y_focus(&mut self, key: Option<String>) {
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
    fn reconcile_a11y_focus(&mut self) {
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
    fn handle_builtin_key(&mut self, event: &KeyEvent) -> bool {
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
    fn handle_builtin_key(&mut self, _event: &KeyEvent) -> bool {
        false
    }
}

/// Буква для Ctrl-комбинации.
///
/// Сначала берётся **физическая** клавиша (`Ctrl`-сочетания не должны зависеть
/// от раскладки: на кириллице `Ctrl+A` даёт логический символ «ф», но физически
/// это по-прежнему клавиша `A`). Если физическая клавиша не распознана, берётся
/// логический символ с разбором управляющих кодов (U+0001..U+001A → `a`..`z`).
fn control_letter(event: &KeyEvent) -> Option<String> {
    if let PhysicalKey::Code(code) = event.physical_key
        && let Some(letter) = keycode_letter(code)
    {
        return Some(letter);
    }
    let Key::Character(ch) = &event.logical_key else {
        return None;
    };
    let mapped: String = ch
        .chars()
        .map(|c| {
            let code = c as u32;
            if (1..=26).contains(&code) {
                char::from(b'a' + (code as u8 - 1))
            } else {
                c
            }
        })
        .collect();
    Some(mapped.to_ascii_lowercase())
}

/// Строчная латинская буква для физической клавиши `A`..`Z` (без зависимости
/// от активной раскладки).
fn keycode_letter(code: KeyCode) -> Option<String> {
    let letter = match code {
        KeyCode::KeyA => 'a',
        KeyCode::KeyB => 'b',
        KeyCode::KeyC => 'c',
        KeyCode::KeyD => 'd',
        KeyCode::KeyE => 'e',
        KeyCode::KeyF => 'f',
        KeyCode::KeyG => 'g',
        KeyCode::KeyH => 'h',
        KeyCode::KeyI => 'i',
        KeyCode::KeyJ => 'j',
        KeyCode::KeyK => 'k',
        KeyCode::KeyL => 'l',
        KeyCode::KeyM => 'm',
        KeyCode::KeyN => 'n',
        KeyCode::KeyO => 'o',
        KeyCode::KeyP => 'p',
        KeyCode::KeyQ => 'q',
        KeyCode::KeyR => 'r',
        KeyCode::KeyS => 's',
        KeyCode::KeyT => 't',
        KeyCode::KeyU => 'u',
        KeyCode::KeyV => 'v',
        KeyCode::KeyW => 'w',
        KeyCode::KeyX => 'x',
        KeyCode::KeyY => 'y',
        KeyCode::KeyZ => 'z',
        _ => return None,
    };
    Some(letter.to_string())
}

/// Формирует [`KeyInfo`] из события клавиатуры.
fn key_info(event: &KeyEvent, modifiers: ModifiersState) -> KeyInfo {
    let key = match &event.logical_key {
        Key::Character(ch) => ch.to_string(),
        Key::Named(named) => format!("{named:?}"),
        _ => String::new(),
    };
    KeyInfo::new(
        key,
        modifiers.shift_key(),
        modifiers.control_key(),
        modifiers.alt_key(),
    )
}

/// Отображение кнопки мыши winit в событие виртуального дерева.
fn map_button(button: MouseButton) -> DomButton {
    match button {
        MouseButton::Left => DomButton::Left,
        MouseButton::Right => DomButton::Right,
        MouseButton::Middle => DomButton::Middle,
        _ => DomButton::Left,
    }
}

/// Координатор цикла событий: набор окон и общие ресурсы.
struct App {
    windows: HashMap<WindowId, WindowState>,
    tokens: HashMap<WindowToken, WindowId>,
    next_token: WindowToken,
    /// Фабрики окон по их `id` (собираются из [`Component::windows`] корня).
    registry: HashMap<String, fn() -> Box<dyn Component>>,
    root: Option<Box<dyn Component>>,
    font: Rc<FontMetrics>,
    error: Option<Error>,
    redraw: Arc<Mutex<Vec<Arc<Window>>>>,
    /// Прокси цикла событий для доставки событий AccessKit.
    #[cfg(feature = "a11y-winit")]
    proxy: Option<EventLoopProxy<UserEvent>>,
}

impl App {
    fn new(root: Box<dyn Component>, font: Rc<FontMetrics>) -> Self {
        let redraw: Arc<Mutex<Vec<Arc<Window>>>> = Arc::new(Mutex::new(Vec::new()));
        let repaint = Arc::clone(&redraw);
        set_repaint_requester(move || {
            if let Ok(list) = repaint.lock() {
                for window in list.iter() {
                    window.request_redraw();
                }
            }
        });
        Self {
            windows: HashMap::new(),
            tokens: HashMap::new(),
            next_token: 0,
            registry: HashMap::new(),
            root: Some(root),
            font,
            error: None,
            redraw,
            #[cfg(feature = "a11y-winit")]
            proxy: None,
        }
    }

    /// Истинно, если окно `token` блокировано открытым модальным потомком.
    ///
    /// Окно блокируется, пока поверх него открыто хотя бы одно дочернее окно
    /// с `modal = true`: все события ввода такого окна игнорируются.
    fn is_blocked(&self, token: WindowToken) -> bool {
        self.windows
            .values()
            .any(|window| window.parent == Some(token) && window.config.modal)
    }

    /// Все потомки окна `token` (по иерархии parent/child), в том числе
    /// косвенные. Возвращаются без самого `token`.
    fn descendants(&self, token: WindowToken) -> Vec<WindowToken> {
        let mut result = Vec::new();
        let mut stack = self
            .windows
            .values()
            .filter(|window| window.parent == Some(token))
            .map(|window| window.token)
            .collect::<Vec<_>>();
        while let Some(next) = stack.pop() {
            result.push(next);
            stack.extend(
                self.windows
                    .values()
                    .filter(|window| window.parent == Some(next))
                    .map(|window| window.token),
            );
        }
        result
    }

    /// Регистрирует окна, объявленные в `windows = [...]` корневого компонента.
    fn build_registry(&mut self, root: &dyn Component) {
        for factory in root.windows() {
            let id = factory().window_config().and_then(|config| config.id);
            if let Some(id) = id {
                self.registry.entry(id).or_insert(factory);
            }
        }
    }

    /// Токен открытого окна с идентификатором `id`.
    fn window_token_by_id(&self, id: &str) -> Option<WindowToken> {
        self.windows
            .values()
            .find(|window| window.config.id.as_deref() == Some(id))
            .map(|window| window.token)
    }

    /// Открывает окно `id` из реестра или фокусирует уже открытое.
    fn open_by_id(&mut self, event_loop: &ActiveEventLoop, id: &str) {
        if let Some(token) = self.window_token_by_id(id) {
            if let Some(window_id) = self.tokens.get(&token).copied()
                && let Some(state) = self.windows.get_mut(&window_id)
            {
                state.window.set_visible(true);
                state.window.focus_window();
                state.invalidate();
            }
            return;
        }
        let Some(factory) = self.registry.get(id).copied() else {
            return;
        };
        let component = factory();
        let config = component.window_config().unwrap_or_default();
        let parent = config
            .parent
            .as_deref()
            .and_then(|parent| self.window_token_by_id(parent));
        if let Err(error) = self.create_window(event_loop, component, parent) {
            self.error = Some(error);
            event_loop.exit();
        }
    }

    /// Закрывает окно с идентификатором `id` (вместе с потомками).
    fn close_by_id(&mut self, event_loop: &ActiveEventLoop, id: &str) {
        if let Some(token) = self.window_token_by_id(id)
            && let Some(window_id) = self.tokens.get(&token).copied()
        {
            self.close_window(event_loop, window_id);
        }
    }

    /// Показывает или скрывает окно с идентификатором `id`.
    fn set_visible_by_id(&mut self, id: &str, visible: bool) {
        if let Some(token) = self.window_token_by_id(id)
            && let Some(window_id) = self.tokens.get(&token).copied()
            && let Some(state) = self.windows.get_mut(&window_id)
        {
            state.window.set_visible(visible);
            state.invalidate();
        }
    }

    fn create_window(
        &mut self,
        event_loop: &ActiveEventLoop,
        component: Box<dyn Component>,
        parent: Option<WindowToken>,
    ) -> crate::Result<WindowToken> {
        let config = component.window_config().unwrap_or_default();
        // Окно создаётся скрытым: адаптер AccessKit обязан быть создан до
        // первого показа. Видимость выставляется ниже, после адаптера.
        let attributes = Window::default_attributes()
            .with_title(&config.title)
            .with_inner_size(LogicalSize::new(config.width, config.height))
            .with_decorations(false)
            .with_transparent(config.transparent)
            .with_resizable(config.resizable)
            .with_window_icon(icon::window_icon(&config))
            .with_visible(false);
        let window = Arc::new(event_loop.create_window(attributes)?);
        if config.min_width.is_some() || config.min_height.is_some() {
            window.set_min_inner_size(Some(LogicalSize::new(
                config.min_width.unwrap_or(1),
                config.min_height.unwrap_or(1),
            )));
        }
        if let (Some(width), Some(height)) = (config.max_width, config.max_height) {
            window.set_max_inner_size(Some(LogicalSize::new(width, height)));
        }
        if let Some(parent_token) = parent
            && let Some(parent_window) = self
                .windows
                .values()
                .find(|state| state.token == parent_token)
            && let Ok(parent_position) = parent_window.window.outer_position()
        {
            let parent_size = parent_window.window.inner_size();
            let child_size = window.outer_size();
            let x = parent_position.x + (parent_size.width as i32 - child_size.width as i32) / 2;
            let y = parent_position.y + (parent_size.height as i32 - child_size.height as i32) / 2;
            window.set_outer_position(PhysicalPosition::new(x, y));
        }
        let size = window.inner_size();
        let width = size.width.max(1);
        let height = size.height.max(1);
        #[cfg(feature = "softbuffer-backend")]
        let surface: Box<dyn RendererBackend> =
            Box::new(SoftbufferBackend::new(width, height, Arc::clone(&window))?);
        #[cfg(all(feature = "wgpu-backend", not(feature = "softbuffer-backend")))]
        let surface: Box<dyn RendererBackend> =
            Box::new(WgpuBackend::new(width, height, Arc::clone(&window))?);
        #[cfg(not(any(feature = "wgpu-backend", feature = "softbuffer-backend")))]
        let surface: Box<dyn RendererBackend> = Box::new(NullBackend::new(width, height));

        #[cfg(feature = "a11y-winit")]
        let adapter = match self.proxy.clone() {
            Some(proxy) if crate::a11y::is_enabled() => Some(
                accesskit_winit::Adapter::with_event_loop_proxy(event_loop, &window, proxy),
            ),
            _ => None,
        };

        let token = self.next_token;
        self.next_token += 1;
        let id = window.id();
        let visible = config.visible;
        if let Ok(mut list) = self.redraw.lock() {
            list.push(Arc::clone(&window));
        }
        let state = WindowState {
            token,
            parent,
            component,
            font: Rc::clone(&self.font),
            window,
            surface,
            config,
            title: String::new(),
            width,
            height,
            scale: 1.0,
            tree: None,
            layout: None,
            full_redraw: true,
            pending_damage: Vec::new(),
            focused: None,
            editing: String::new(),
            caret: 0,
            anchor: None,
            dragging: false,
            resizing: None,
            resized: HashMap::new(),
            scroll: HashMap::new(),
            validation: HashMap::new(),
            modifiers: ModifiersState::empty(),
            cursor: Point::new(0.0, 0.0),
            hover: None,
            hover_top: None,
            menu: None,
            clipboard: String::new(),
            undo: Vec::new(),
            redo: Vec::new(),
            cursor_icon: None,
            last_click: None,
            #[cfg(feature = "elements-select")]
            open_select: None,
            #[cfg(feature = "elements-select")]
            select_hover: None,
            #[cfg(feature = "a11y-winit")]
            adapter,
            #[cfg(feature = "a11y-winit")]
            a11y_focus: None,
        };
        self.tokens.insert(token, id);
        self.windows.insert(id, state);
        if let Some(parent) = parent
            && let Some(parent_id) = self.tokens.get(&parent).copied()
            && let Some(parent_state) = self.windows.get_mut(&parent_id)
        {
            parent_state.invalidate();
        }
        if let Some(window) = self.windows.get(&id) {
            if visible {
                window.window.set_visible(true);
            }
            window.window.request_redraw();
        }
        Ok(token)
    }

    fn close_window(&mut self, event_loop: &ActiveEventLoop, id: WindowId) {
        let Some(state) = self.windows.get(&id) else {
            return;
        };
        let token = state.token;
        let parent = state.parent;
        let mut closing = self.descendants(token);
        closing.push(token);
        for token in closing {
            let Some(child_id) = self.tokens.remove(&token) else {
                continue;
            };
            crate::dom::remove_tree(token);
            if let Some(state) = self.windows.remove(&child_id) {
                state.window.set_visible(false);
            }
            if let Ok(mut list) = self.redraw.lock() {
                list.retain(|window| window.id() != child_id);
            }
        }
        if let Some(parent) = parent
            && let Some(parent_id) = self.tokens.get(&parent).copied()
            && let Some(parent_state) = self.windows.get_mut(&parent_id)
        {
            parent_state.invalidate();
        }
        if !self.windows.values().any(|window| window.parent.is_none()) {
            event_loop.exit();
        }
    }

    fn open_modal(&mut self, event_loop: &ActiveEventLoop, token: WindowToken) {
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
    fn open_window(&mut self, event_loop: &ActiveEventLoop, token: WindowToken) {
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

    fn apply_command(&self, token: WindowToken, command: WindowCommand) {
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
    /// Реагирует на событие адаптера AccessKit: отдаёт начальное дерево,
    /// выполняет запрошенное диктором действие или игнорирует деактивацию.
    #[cfg(feature = "a11y-winit")]
    fn on_access_event(&mut self, event: accesskit_winit::Event) {
        let Some(state) = self.windows.get_mut(&event.window_id) else {
            return;
        };
        match event.window_event {
            accesskit_winit::WindowEvent::InitialTreeRequested => {
                state.push_accessibility();
            }
            accesskit_winit::WindowEvent::ActionRequested(request) => {
                state.handle_action_request(&request);
            }
            accesskit_winit::WindowEvent::AccessibilityDeactivated => {}
        }
    }
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if !self.windows.is_empty() {
            return;
        }
        let Some(component) = self.root.take() else {
            return;
        };
        self.build_registry(&*component);
        if let Err(error) = self.create_window(event_loop, component, None) {
            self.error = Some(error);
            event_loop.exit();
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        let (token, command) = match event {
            UserEvent::Command { token, command } => (token, command),
            #[cfg(feature = "a11y-winit")]
            UserEvent::Access(event) => {
                self.on_access_event(event);
                return;
            }
        };
        match command {
            WindowCommand::OpenModal => self.open_modal(event_loop, token),
            WindowCommand::OpenWindow => self.open_window(event_loop, token),
            WindowCommand::Close => {
                if let Some(id) = self.tokens.get(&token).copied() {
                    self.close_window(event_loop, id);
                }
            }
            WindowCommand::Open(id) => self.open_by_id(event_loop, &id),
            WindowCommand::CloseWindow(id) => self.close_by_id(event_loop, &id),
            WindowCommand::ShowWindow(id) => self.set_visible_by_id(&id, true),
            WindowCommand::HideWindow(id) => self.set_visible_by_id(&id, false),
            _ => self.apply_command(token, command),
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(token) = self.windows.get(&id).map(|window| window.token) else {
            return;
        };
        #[cfg(feature = "a11y-winit")]
        if let Some(state) = self.windows.get_mut(&id)
            && let Some(adapter) = state.adapter.as_mut()
        {
            adapter.process_event(&state.window, &event);
        }
        if let WindowEvent::CloseRequested = event {
            self.close_window(event_loop, id);
            return;
        }
        let is_input = matches!(
            &event,
            WindowEvent::CursorMoved { .. }
                | WindowEvent::MouseInput { .. }
                | WindowEvent::KeyboardInput { .. }
                | WindowEvent::ModifiersChanged(_)
                | WindowEvent::MouseWheel { .. }
        );
        if is_input && self.is_blocked(token) {
            return;
        }
        let Some(window) = self.windows.get_mut(&id) else {
            return;
        };
        set_active_window(token);
        match event {
            WindowEvent::Resized(_) => window.invalidate(),
            WindowEvent::Focused(false) => window.on_unfocus(),
            WindowEvent::ModifiersChanged(modifiers) => window.modifiers = modifiers.state(),
            WindowEvent::CursorMoved { position, .. } => {
                window.on_cursor_moved(position.x as f32, position.y as f32);
            }
            WindowEvent::CursorLeft { .. } => window.on_cursor_left(),
            WindowEvent::MouseInput { state, button, .. } => window.on_mouse(state, button),
            WindowEvent::KeyboardInput { event, .. } => window.on_key(&event),
            WindowEvent::MouseWheel { delta, .. } => window.on_wheel(delta),
            WindowEvent::RedrawRequested => {
                if let Err(error) = window.redraw() {
                    self.error = Some(error);
                    event_loop.exit();
                }
            }
            _ => {}
        }
    }
}

impl WindowState {
    /// Снимает фокус при потере активности окна.
    fn on_unfocus(&mut self) {
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
    fn on_wheel(&mut self, delta: MouseScrollDelta) {
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
    fn on_mouse(&mut self, state: ElementState, button: MouseButton) {
        match state {
            ElementState::Pressed => {
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

/// Применяет сохранённые размеры полей (`resize`) к дереву перед раскладкой.
fn apply_overrides(tree: &mut VElement, resized: &HashMap<&'static str, (f32, f32)>) {
    for (&key, &(width, height)) in resized {
        if let Some(node) = tree.get_mut(key) {
            node.style.width = crate::vdom::Size::Px(width);
            node.style.height = crate::vdom::Size::Px(height);
        }
    }
}

/// Помечает невалидные поля красной рамкой и добавляет текст ошибки для
/// диктора (`aria-invalid`/`aria-description`) поверх дерева компонента.
fn apply_validation(tree: &mut VElement, validation: &HashMap<String, String>) {
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
fn upsert_attr(node: &mut VElement, name: &'static str, value: String) {
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
fn word_range(text: &str, index: usize) -> (usize, usize) {
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

/// Преобразует курсор Light в иконку курсора окна.
fn cursor_icon(cursor: Cursor) -> CursorIcon {
    match cursor {
        Cursor::Default => CursorIcon::Default,
        Cursor::Pointer => CursorIcon::Pointer,
        Cursor::Text => CursorIcon::Text,
        Cursor::NotAllowed => CursorIcon::NotAllowed,
        Cursor::Wait => CursorIcon::Wait,
        Cursor::Progress => CursorIcon::Progress,
        Cursor::Help => CursorIcon::Help,
        Cursor::Crosshair => CursorIcon::Crosshair,
        Cursor::Move => CursorIcon::Move,
        Cursor::Grab => CursorIcon::Grab,
        Cursor::Grabbing => CursorIcon::Grabbing,
        Cursor::EwResize => CursorIcon::EwResize,
        Cursor::NsResize => CursorIcon::NsResize,
        Cursor::NeswResize => CursorIcon::NeswResize,
        Cursor::NwseResize => CursorIcon::NwseResize,
    }
}

/// Курсор, который узел задаёт сам (`cursor="…"`), либо `None`.
fn explicit_cursor(element: &VElement) -> Option<Cursor> {
    element.attr("cursor").and_then(Cursor::from_name)
}

/// Похож ли узел на ссылку: `role="link"` или HTML-тег `a`.
fn is_link(element: &VElement) -> bool {
    element.tag.eq_ignore_ascii_case("a")
        || element
            .attr("role")
            .is_some_and(|role| role.eq_ignore_ascii_case("link"))
}

/// Курсор по умолчанию для узла, если атрибут `cursor` не задан.
///
/// Отключённые элементы показывают `not-allowed`, ссылки и элементы управления
/// — `pointer`, поля ввода — `text`, остальные — системный курсор.
pub fn default_cursor_for(element: &VElement) -> Cursor {
    if crate::a11y::is_disabled(element) {
        return Cursor::NotAllowed;
    }
    if is_link(element) {
        return Cursor::Pointer;
    }
    match &element.kind {
        NodeKind::Button { .. } => Cursor::Pointer,
        #[cfg(feature = "elements-checkbox")]
        NodeKind::Checkbox { .. } => Cursor::Pointer,
        #[cfg(feature = "elements-radio")]
        NodeKind::Radio { .. } => Cursor::Pointer,
        #[cfg(feature = "elements-select")]
        NodeKind::Select { .. } => Cursor::Pointer,
        NodeKind::TextField { .. } => Cursor::Text,
        _ => Cursor::Default,
    }
}

/// Курсор для узла: явный `cursor`, иначе значение по роли.
pub fn cursor_for(element: &VElement) -> Cursor {
    explicit_cursor(element).unwrap_or_else(|| default_cursor_for(element))
}

/// Определяет курсор для точки окна.
///
/// Узлы перебираются от самого глубокого к корню (`hit_stack`), поэтому
/// значение может «всплывать»: если у дочернего узла нет собственного курсора
/// (он возвращает `Default`), берётся курсор ближайшего подходящего родителя.
/// Явно заданный `cursor="default"` прекращает всплытие.
pub fn resolve_cursor(layout: &Layout, tree: &VElement, point: Point) -> Cursor {
    for node in layout.hit_stack(point) {
        let Some(element) = tree.get(node.key) else {
            continue;
        };
        if let Some(cursor) = explicit_cursor(element) {
            return cursor;
        }
        let cursor = default_cursor_for(element);
        if cursor != Cursor::Default {
            return cursor;
        }
    }
    Cursor::Default
}

#[cfg(test)]
mod tests {
    use super::{keycode_letter, word_range};
    use winit::keyboard::KeyCode;

    #[test]
    fn keycode_letter_is_layout_independent() {
        assert_eq!(keycode_letter(KeyCode::KeyA).as_deref(), Some("a"));
        assert_eq!(keycode_letter(KeyCode::KeyX).as_deref(), Some("x"));
        assert_eq!(keycode_letter(KeyCode::KeyZ).as_deref(), Some("z"));
        assert_eq!(keycode_letter(KeyCode::Digit1), None);
        assert_eq!(keycode_letter(KeyCode::Escape), None);
    }

    #[test]
    fn word_range_selects_word_under_index() {
        assert_eq!(word_range("hello world", 2), (0, 5));
        assert_eq!(word_range("hello world", 8), (6, 11));
        assert_eq!(word_range("hello world", 0), (0, 5));
        assert_eq!(word_range("hello world", 11), (6, 11));
        // Позиция сразу после слова «прилипает» к нему.
        assert_eq!(word_range("hello world", 5), (0, 5));
        assert_eq!(word_range("hello,world", 5), (0, 5));
        // Изолированный несловесный символ выделяется сам.
        assert_eq!(word_range("a  b", 2), (2, 3));
        assert_eq!(word_range("", 0), (0, 0));
        assert_eq!(word_range("word", 4), (0, 4));
        assert_eq!(word_range("_x_9", 1), (0, 4));
    }
}
