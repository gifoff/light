use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use light_reactivity::set_repaint_requester;
use light_vdom::component::Component;
use light_vdom::window_backend::{WindowToken, set_command_sender};
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::window::{Window, WindowId};

use crate::Error;

use super::font::FontMetrics;
use super::geometry::WindowGeometry;
use super::state::WindowState;

pub(crate) type UserEvent = crate::async_runtime::UserEvent;
#[cfg(not(feature = "async-runtime"))]
pub(crate) enum UserEvent {
    Command {
        token: WindowToken,
        command: WindowCommand,
    },
    #[cfg(feature = "a11y-winit")]
    Access(accesskit_winit::Event),
}

#[cfg(all(not(feature = "async-runtime"), feature = "a11y-winit"))]
impl From<accesskit_winit::Event> for UserEvent {
    pub(super) fn from(event: accesskit_winit::Event) -> Self {
        Self::Access(event)
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
        app.proxy = Some(proxy.clone());
    }
    #[cfg(feature = "async-runtime")]
    {
        crate::async_runtime::install_proxy(event_loop.create_proxy())
            .expect("winit: прокси UI-цикла устанавливается один раз");
    }
    // Команды DevTools (CLICK) доставляем через прокси: он будит спящий
    // цикл событий, и клик обрабатывается сразу, а не при следующей
    // перерисовке. Без него TCP-сервер ответил бы `timeout`.
    #[cfg(all(feature = "devtools", feature = "async-runtime"))]
    {
        crate::devtools::install_proxy(&proxy);
    }
    let result = event_loop.run_app(&mut app);
    // ТЗ 0.14.3: сохраняем накопленную геометрию при выходе.
    app.save_geometry_on_exit();
    result?;
    match app.error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

pub(super) struct App {
    pub(super) windows: HashMap<WindowId, WindowState>,
    pub(super) tokens: HashMap<WindowToken, WindowId>,
    pub(super) next_token: WindowToken,
    /// Фабрики окон по их `id` (собираются из [`Component::windows`] корня).
    pub(super) registry: HashMap<String, fn() -> Box<dyn Component>>,
    pub(super) root: Option<Box<dyn Component>>,
    pub(super) font: Rc<FontMetrics>,
    pub(super) error: Option<Error>,
    pub(super) redraw: Arc<Mutex<Vec<Arc<Window>>>>,
    /// Геометрия корневого окна для seamless-swap: пишется с дебаунсом 500 мс
    /// в `.light/window.json`, читается при следующем запуске.
    pub(super) window_geom: Option<WindowGeometry>,
    pub(super) window_geom_dirty: bool,
    pub(super) window_geom_last_write: std::time::Instant,
    /// ТЗ 0.14.3-fix7: последняя «нормальная» (не minimized, не off-screen)
    /// геометрия. Когда окно свёрнуто, вместо мусорных координат Windows
    /// `(-32000, -32000)` в файл уходит именно эта — чтобы restore не ломал
    /// thumbnail/видимость.
    pub(super) cached_normal_geometry: Option<WindowGeometry>,
    /// ТЗ 0.14.3-fix8: установлено при CloseRequested (WM_CLOSE / X-кнопка).
    /// В `save_geometry_on_exit` предотвращает запись geometry-файла при
    /// user close: supervisor сам удалит файлы. При swap (`LIGHT_SWAP=1`)
    /// файл **сохраняется** — следующий child восстановит позицию.
    pub(super) closing: bool,
    /// ТЗ 0.15.1-fix2: флаг «идёт восстановление геометрии». Когда мы
    /// применяем сохранённую позицию через `set_outer_position`, Windows
    /// может скорректировать её (workspace boundary, taskbar и т.п.) и
    /// сгенерировать `WindowEvent::Moved` со сдвинутыми координатами.
    /// Без этого флага `flush_geometry` закэширует WM-коррекцию, и
    /// следующий swap применит уже сдвинутую позицию → drift.
    /// Флаг живёт ~200 мс (пока WM не доставит Moved-события от нашего
    /// `set_outer_position`).
    pub(super) restoring_geometry: bool,
    pub(super) restore_started_at: Option<std::time::Instant>,
    /// ТЗ 0.15.1-fix3: «желаемая» геометрия — то, что мы применили через
    /// `set_outer_position` / `request_inner_size`, а не то, что видит WM
    /// (WM может скорректировать frame offset и т.п.). При сохранении
    /// (`flush_geometry`) пишем именно её в `window.json`, чтобы при swap
    /// применялась исходная позиция без накопления drift. Обновляется:
    /// * в restore-ветке `apply_geometry` — полным клон из файла;
    /// * в `WindowEvent::Moved` (не во время restore) — x/y;
    /// * в `WindowEvent::Resized` (не во время restore) — width/height.
    pub(super) last_applied_geometry: Option<WindowGeometry>,
    /// Прокси цикла событий для доставки событий AccessKit.
    #[cfg(feature = "a11y-winit")]
    pub(super) proxy: Option<EventLoopProxy<UserEvent>>,
}

impl App {
    pub(super) fn new(root: Box<dyn Component>, font: Rc<FontMetrics>) -> Self {
        let redraw: Arc<Mutex<Vec<Arc<Window>>>> = Arc::new(Mutex::new(Vec::new()));
        let repaint = Arc::clone(&redraw);
        set_repaint_requester(move || {
            if let Ok(list) = repaint.lock() {
                for window in list.iter() {
                    window.request_redraw();
                }
            }
        });
        // Токен `0` зарезервирован под «окно по умолчанию» (см.
        // [`light_vdom::window_backend::current_window_token`]: возвращается `None`).
        // Поэтому нумерация реальных окон начинается с `1`.
        Self {
            windows: HashMap::new(),
            tokens: HashMap::new(),
            next_token: 1,
            registry: HashMap::new(),
            root: Some(root),
            font,
            error: None,
            redraw,
            window_geom: None,
            window_geom_dirty: false,
            window_geom_last_write: std::time::Instant::now(),
            cached_normal_geometry: None,
            closing: false,
            restoring_geometry: false,
            restore_started_at: None,
            last_applied_geometry: None,
            #[cfg(feature = "a11y-winit")]
            proxy: None,
        }
    }

    /// Истинно, если окно `token` блокировано открытым модальным потомком.
    ///
    /// Окно блокируется, пока поверх него открыто хотя бы одно дочернее окно
    /// с `modal = true`: все события ввода такого окна игнорируются.
    pub(super) fn is_blocked(&self, token: WindowToken) -> bool {
        self.windows
            .values()
            .any(|window| window.parent == Some(token) && window.config.modal)
    }

    /// Все потомки окна `token` (по иерархии parent/child), в том числе
    /// косвенные. Возвращаются без самого `token`.
    pub(super) fn descendants(&self, token: WindowToken) -> Vec<WindowToken> {
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
    pub(super) fn build_registry(&mut self, root: &dyn Component) {
        for factory in root.windows() {
            let id = factory().window_config().and_then(|config| config.id);
            if let Some(id) = id {
                self.registry.entry(id).or_insert(factory);
            }
        }
    }

    /// Токен открытого окна с идентификатором `id`.
    pub(super) fn window_token_by_id(&self, id: &str) -> Option<WindowToken> {
        self.windows
            .values()
            .find(|window| window.config.id.as_deref() == Some(id))
            .map(|window| window.token)
    }

    /// Открывает окно `id` из реестра или фокусирует уже открытое.
    pub(super) fn open_by_id(&mut self, event_loop: &ActiveEventLoop, id: &str) {
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
    pub(super) fn close_by_id(&mut self, event_loop: &ActiveEventLoop, id: &str) {
        if let Some(token) = self.window_token_by_id(id)
            && let Some(window_id) = self.tokens.get(&token).copied()
        {
            self.close_window(event_loop, window_id);
        }
    }

    /// Показывает или скрывает окно с идентификатором `id`.
    pub(super) fn set_visible_by_id(&mut self, id: &str, visible: bool) {
        if let Some(token) = self.window_token_by_id(id)
            && let Some(window_id) = self.tokens.get(&token).copied()
            && let Some(state) = self.windows.get_mut(&window_id)
        {
            state.window.set_visible(visible);
            state.invalidate();
        }
    }
}



