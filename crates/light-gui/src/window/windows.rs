use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use light_vdom::component::Component;
use light_vdom::layout::Point;
use light_vdom::window_backend::WindowToken;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::ModifiersState;
use winit::window::{Window, WindowId};

#[cfg(not(any(feature = "wgpu-backend", feature = "softbuffer-backend")))]
use crate::render::NullBackend;
use crate::render::{RendererBackend, SoftbufferBackend};

use super::app::{App, UserEvent};
use super::geometry::WindowGeometry;
#[allow(unused_imports)]
use super::icon;
use super::keys::is_resume_from_swap;
use super::state::WindowState;



impl App {
    pub(super) fn create_window(
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
        // ТЗ 0.14.3: применяем сохранённую геометрию (position/size/maximized)
        // до того, как дочернее центрирование или размер от config вступят в
        // силу. Применяется только для корневого окна (parent == None).
        //
        // ТЗ 0.14.3-fix7: восстанавливаем ТОЛЬКО при hot-swap
        // (LIGHT_RESUME_FROM_SWAP=1). На cold start окно остаётся в дефолтном
        // центре экрана — устаревшая geometry из прошлой сессии (включая
        // мусорные (-32000, -32000) от свёрнутого окна) не применяется. Мусорную
        // геометрию (x/y < -10000) пропускаем и на swap; minimized НЕ ставим —
        // это ломает thumbnail (окно спавним normal, при maximized — set_maximized
        // после show).
        if parent.is_none() {
            let is_swap = is_resume_from_swap();
            let loaded_geom = self
                .window_geom
                .as_ref()
                .cloned()
                .or_else(WindowGeometry::load);
            if !is_swap {
                // Отладочный лог — только в debug-сборке (ТЗ 0.17.6).
                if cfg!(debug_assertions) {
                    eprintln!("light: cold start — geometry not restored");
                }
            } else if let Some(geom) = &loaded_geom
                && !geom.is_garbage()
            {
                // ТЗ 0.15.1-fix2: помечаем «восстановление геометрии», чтобы
                // `flush_geometry` пропустил Moved-события от WM-коррекции
                // (иначе drift: 157,156 → 164,160 → … после каждого swap).
                self.restoring_geometry = true;
                self.restore_started_at = Some(std::time::Instant::now());
                // ТЗ 0.15.1-fix3: запоминаем «желаемую» геометрию — именно её
                // будем писать в файл при flush (не WM-коррекцию).
                self.last_applied_geometry = Some(geom.clone());
                #[cfg(feature = "devtools")]
                {
                    eprintln!(
                        "[apply] desired=({}, {}, {}x{}) outer_pos={:?} inner_pos={:?}",
                        geom.x,
                        geom.y,
                        geom.width,
                        geom.height,
                        window.outer_position(),
                        window.inner_position(),
                    );
                }
                let _ = window
                    .request_inner_size(LogicalSize::new(geom.width as f64, geom.height as f64));
                window.set_outer_position(PhysicalPosition::new(geom.x, geom.y));
                // ТЗ 0.14.3-fix9: maximized и minimized взаимоисключающие.
                if geom.maximized && !geom.minimized {
                    window.set_maximized(true);
                }
                // ТЗ 0.14.3-fix9: при minimized — спавним видимым (чтобы
                // первый frame отрисовался и thumbnail показал содержимое),
                // затем через 100 мс сворачиваем через DeferredMinimize.
                if geom.minimized {
                    // ТЗ 0.14.3-fix9: спавним видимым (thumbnail), через
                    // 100 мс сворачиваем. Прокси берём из UI_PROXY статика.
                    #[cfg(feature = "async-runtime")]
                    {
                        let window_id = window.id();
                        std::thread::spawn(move || {
                            std::thread::sleep(std::time::Duration::from_millis(100));
                            if let Some(proxy) = crate::async_runtime::UI_PROXY.get() {
                                let _ = proxy.send_event(UserEvent::DeferredMinimize { window_id });
                            } else {
                                eprintln!("light: UI proxy not installed, cannot defer minimize");
                            }
                        });
                        eprintln!("light: deferred minimize scheduled");
                    }
                }
                eprintln!(
                    "light: restored window geometry ({}, {}, {}x{}, maximized={}, minimized={})",
                    geom.x, geom.y, geom.width, geom.height, geom.maximized, geom.minimized
                );
                self.window_geom = Some(geom.clone());
                if !geom.maximized {
                    self.cached_normal_geometry = Some(geom.clone());
                }
            } else if loaded_geom.as_ref().is_some_and(|g| g.is_garbage()) {
                eprintln!(
                    "light: saved geometry is off-screen (minimized) — using default position"
                );
            }
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
            scroll_drag: None,
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
            custom_instances: HashMap::new(),
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

    /// Обновляет и (при необходимости) сохраняет геометрию корневого окна.
    /// Вызывается при каждом `Moved`/`Resized` событии; запись в файл
    /// дебаунсится на 500 мс.
    pub(super) fn flush_geometry(&mut self, id: WindowId) {
        // ТЗ 0.15.1-fix2: не кэшируем позицию во время восстановления
        // геометрии — это WM-коррекция от нашего `set_outer_position`, а не
        // движение пользователя. Иначе следующий swap применит сдвинутую
        // координату (drift).
        if self.restoring_geometry {
            return;
        }
        let Some(state) = self.windows.get(&id) else {
            return;
        };
        // Пишем только для корневого окна (parent == None).
        if state.parent.is_some() {
            return;
        }
        // ТЗ 0.15.1-fix3: используем «желаемую» геометрию (то, что мы
        // применили через set_outer_position/request_inner_size), а не
        // WM-наблюдаемые координаты outer_position(). WM может скорректировать
        // frame offset, и чтение его значения накапливает drift при каждом
        // swap. Фолбэк на outer_position(), если last_applied ещё не задан.
        let pos = if let Some(applied) = &self.last_applied_geometry {
            (applied.x, applied.y)
        } else {
            let Ok(p) = state.window.outer_position() else {
                return;
            };
            (p.x, p.y)
        };
        #[cfg(feature = "devtools")]
        {
            eprintln!(
                "[save] source={}, geom=({}, {}, {}x{})",
                if self.last_applied_geometry.is_some() {
                    "last_applied"
                } else {
                    "outer_position"
                },
                pos.0,
                pos.1,
                state.window.inner_size().width,
                state.window.inner_size().height,
            );
        }
        // ТЗ 0.14.3-fix7: свёрнутое окно имеет мусорные координаты Windows
        // (например (-32000, -32000)) + уменьшенный размер. Не записываем их:
        // в файл уходит последняя «нормальная» геометрия из кэша, чтобы restore
        // не ставил окно off-screen и thumbnail остался живым.
        if state.window.is_minimized().unwrap_or(false) {
            if let Some(last) = &self.cached_normal_geometry {
                // ТЗ 0.14.3-fix9: сохраняем нормальную геометрию + minimized=true,
                // чтобы swap-restore мог применить deferred minimize.
                let mut geom = last.clone();
                geom.minimized = true;
                self.window_geom = Some(geom.clone());
                self.window_geom_dirty = true;
                let now = std::time::Instant::now();
                if now.duration_since(self.window_geom_last_write)
                    >= std::time::Duration::from_millis(500)
                {
                    if let Err(error) = geom.save() {
                        eprintln!("light: не удалось сохранить геометрию: {error}");
                    } else {
                        eprintln!(
                            "light: geometry saved (minimized=true, x={}, y={})",
                            geom.x, geom.y
                        );
                    }
                    self.window_geom_dirty = false;
                    self.window_geom_last_write = now;
                }
            }
            return;
        }
        let size = state.window.inner_size();
        let minimized = state.window.is_minimized().unwrap_or(false);
        let geom = WindowGeometry {
            x: pos.0,
            y: pos.1,
            width: size.width,
            height: size.height,
            maximized: state.window.is_maximized(),
            minimized,
        };
        self.window_geom = Some(geom.clone());
        self.window_geom_dirty = true;
        // Кэшируем нормальную геометрию (только когда окно не maximized),
        // чтобы при сворачивании была что восстановить.
        if !geom.maximized {
            self.cached_normal_geometry = Some(geom.clone());
        }
        let now = std::time::Instant::now();
        if now.duration_since(self.window_geom_last_write) >= std::time::Duration::from_millis(500)
        {
            if let Err(error) = geom.save() {
                eprintln!("light: не удалось сохранить геометрию: {error}");
            }
            self.window_geom_dirty = false;
            self.window_geom_last_write = now;
        }
    }

    /// ТЗ 0.15.1-fix3: принудительная запись геометрии в файл без дебаунса.
    /// Вызывается из обработчика `SAVE_ALL` перед swap-kill: supervisor уже
    /// заберёт процесс через TerminateProcess, и обычная 500 мс пауза может
    /// не успеть сработать. Пишем то, что накопилось в `window_geom`
    /// (обновлённое `last_applied_geometry`), и сбрасываем dirty-флаг.
    #[cfg_attr(
        not(all(feature = "devtools", feature = "async-runtime")),
        allow(dead_code)
    )]
    pub(super) fn flush_geometry_force(&mut self, id: WindowId) {
        // Только корневое окно.
        let Some(state) = self.windows.get(&id) else {
            return;
        };
        if state.parent.is_some() {
            return;
        }
        // Строим геометрию из last_applied + текущий size/maximized.
        let pos = if let Some(applied) = &self.last_applied_geometry {
            (applied.x, applied.y)
        } else {
            let Ok(p) = state.window.outer_position() else {
                return;
            };
            (p.x, p.y)
        };
        let size = state.window.inner_size();
        let geom = WindowGeometry {
            x: pos.0,
            y: pos.1,
            width: size.width,
            height: size.height,
            maximized: state.window.is_maximized(),
            minimized: state.window.is_minimized().unwrap_or(false),
        };
        self.window_geom = Some(geom.clone());
        if !geom.maximized {
            self.cached_normal_geometry = Some(geom.clone());
        }
        #[cfg(feature = "devtools")]
        {
            eprintln!(
                "[save] force: source=last_applied, geom=({}, {}, {}x{})",
                geom.x, geom.y, geom.width, geom.height
            );
        }
        if let Err(error) = geom.save() {
            eprintln!("light: не удалось принудительно сохранить геометрию: {error}");
        } else {
            self.window_geom_dirty = false;
            self.window_geom_last_write = std::time::Instant::now();
        }
    }

    /// Сохраняет накопленную геометрию (вызывается при выходе).
    pub(super) fn save_geometry_on_exit(&self) {
        // ТЗ 0.14.3-fix8: при user close не пишем geometry-файл —
        // supervisor сам удалит оба файла при чистом выходе child'а.
        // При swap (LIGHT_SWAP=1) файл **сохраняется**: следующий child
        // восстановит позицию окна из него.
        if self.closing && std::env::var("LIGHT_SWAP").is_err() {
            eprintln!("light: geometry save skipped (user close, supervisor will clean up)");
            return;
        }
        if self.window_geom_dirty
            && let Some(geom) = &self.window_geom
            && let Err(error) = geom.save()
        {
            eprintln!("light: не удалось сохранить геометрию при выходе: {error}");
        }
    }

    pub(super) fn close_window(&mut self, event_loop: &ActiveEventLoop, id: WindowId) {
        let Some(state) = self.windows.get(&id) else {
            return;
        };
        let token = state.token;
        let parent = state.parent;
        // При закрытии верхнего окна сохраняем состояние компонента в файл
        // `.light/state.json` (фича `devtools`). Запись асинхронна от UI:
        // ошибка лишь логируется — не блокируем выход приложения. Токены
        // дочерних окон не перезаписываются до цикла удаления ниже, поэтому
        // здесь все окна ещё на месте.
        #[cfg(feature = "devtools")]
        if state.parent.is_none()
            && let Err(error) = crate::devtools::save_state_file(&state.component.inspect_state())
        {
            eprintln!("DevTools: не удалось сохранить состояние: {error}");
        }
        // ТЗ 0.14.3-fix3: при clean exit (не swap) удаляем state.json,
        // чтобы следующее запуск стартовал с чистым состоянием.
        // ТЗ 0.14.3-fix4: при --preserve-state (LIGHT_PRESERVE_STATE=1)
        // state.json сохраняется даже при обычном выходе.
        #[cfg(feature = "devtools")]
        if state.parent.is_none() {
            let is_swap = std::env::var("LIGHT_SWAP").is_ok();
            let is_preserve = std::env::var("LIGHT_PRESERVE_STATE").is_ok();
            if !is_swap && !is_preserve {
                let state_path = crate::devtools::state_file_path();
                if std::fs::remove_file(&state_path).is_ok() {
                    eprintln!("light: clean exit — state.json removed");
                }
            } else if is_preserve {
                eprintln!("light: clean exit with --preserve-state — state.json kept");
            } else {
                eprintln!("light: clean exit during swap — state.json kept");
            }
        }
        let mut closing = self.descendants(token);
        closing.push(token);
        for token in closing {
            let Some(child_id) = self.tokens.remove(&token) else {
                continue;
            };
            light_vdom::dom::remove_tree(token);
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
}
