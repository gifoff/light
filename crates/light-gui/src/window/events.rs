use super::app::{App, UserEvent};
use super::geometry::WindowGeometry;
use light_vdom::window_backend::{WindowCommand, set_active_window, set_current_window_token};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::WindowId;

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if !self.windows.is_empty() {
            return;
        }
        let component = match self.root.take() {
            Some(c) => c,
            None => return,
        };
        // ТЗ 0.14.3-fix4: применяем state.json ТОЛЬКО при hot-swap
        // (LIGHT_RESUME_FROM_SWAP=1) или явном --preserve-state
        // (LIGHT_PRESERVE_STATE=1). На cold start дефолты из кода
        // имеют приоритет над устаревшим state.
        #[cfg(not(feature = "devtools"))]
        let component = component;
        #[cfg(feature = "devtools")]
        let component = {
            let mut c = component;
            let is_resume_swap = std::env::var("LIGHT_RESUME_FROM_SWAP").is_ok();
            let is_preserve = std::env::var("LIGHT_PRESERVE_STATE").is_ok();
            if is_resume_swap || is_preserve {
                match crate::devtools::load_state_file() {
                    Ok(Some(state)) => {
                        let fields = state.as_object().map(|m| m.len()).unwrap_or(0);
                        c.inject_state(&state);
                        if is_preserve {
                            eprintln!(
                                "light: state loaded ({fields} fields injected) [preserve-state]"
                            );
                        } else {
                            eprintln!(
                                "light: state loaded ({fields} fields injected) [resume from swap]"
                            );
                        }
                    }
                    Ok(None) => {
                        eprintln!(
                            "light: no state file at {:?}",
                            crate::devtools::state_file_path()
                        );
                    }
                    Err(_) => {
                        eprintln!(
                            "light: no state file at {:?}",
                            crate::devtools::state_file_path()
                        );
                    }
                }
            } else {
                // Отладочный лог — только в debug-сборке (ТЗ 0.17.6).
                if cfg!(debug_assertions) {
                    eprintln!("light: cold start — state.json not applied");
                }
            }
            c
        };
        self.build_registry(&*component);
        if let Err(error) = self.create_window(event_loop, component, None) {
            self.error = Some(error);
            event_loop.exit();
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        #[cfg(all(feature = "devtools", feature = "async-runtime"))]
        if let crate::async_runtime::UserEvent::DevTools(cmd) = event {
            self.handle_devtools_command(cmd);
            // Перерисовываем все открытые окна: состояние могло измениться в
            // любом из них, а `invalidate()` вызывается только для окна, к
            // которому применили команду. Так инспектор сразу увидит результат.
            for window in self.windows.values_mut() {
                window.window.request_redraw();
            }
            return;
        }
        // ТЗ 0.14.3-fix9: отложенный minimize — применяем после того как
        // первый frame уже отрисован (thumbnail содержит содержимое).
        #[cfg(feature = "async-runtime")]
        if let UserEvent::DeferredMinimize { window_id } = event {
            if let Some(state) = self.windows.get(&window_id) {
                state.window.set_minimized(true);
                eprintln!("light: deferred minimize applied");
            }
            return;
        }
        let (token, command) = match event {
            UserEvent::Command { token, command } => (token, command),
            #[cfg(feature = "a11y-winit")]
            UserEvent::Access(event) => {
                self.on_access_event(event);
                return;
            }
            #[cfg(feature = "async-runtime")]
            UserEvent::UpdateUI(f) => {
                f();
                for window in self.windows.values_mut() {
                    window.window.request_redraw();
                }
                return;
            }
            #[cfg(feature = "async-runtime")]
            UserEvent::Action { f, target, token } => {
                // Окно находится по токену (или по TypeId, если токен не
                // задан) внутри apply_action. Функция возвращает токен окна,
                // к которому действие было применено; после возврата из неё
                // borrow на self.windows снят, поэтому безопасно разыскиваем
                // окно по токену и запрашиваем его перерисовку — ровно как в
                // прежнем обработчике.
                let matched = crate::async_runtime::apply_action(
                    self.windows.iter_mut().map(|(_id, state)| {
                        (
                            state.token,
                            state.component.as_any_mut().type_id(),
                            state.component.as_mut(),
                        )
                    }),
                    token,
                    target,
                    f,
                );
                match matched {
                    Some(m) => {
                        if let Some(id) = self.tokens.get(&m).copied()
                            && let Some(state) = self.windows.get(&id)
                        {
                            state.window.request_redraw();
                        }
                    }
                    None => eprintln!("async-runtime: target window closed"),
                }
                return;
            }
            // Ветка `UserEvent::DevTools` обрабатывается выше ранним возвратом
            // (`handle_devtools_command`); здесь лишь обеспечиваем полноту
            // матча в конфигурации `devtools` + `async-runtime`, где ранний
            // возврат уже перехватил такое событие (ветка недостижима).
            #[cfg(all(feature = "devtools", feature = "async-runtime"))]
            UserEvent::DevTools(_) => {
                unreachable!("devtools-событие перехватывается ранним возвратом выше")
            }
            // ТЗ 0.14.3-fix9: обработано ранним возвратом выше.
            #[cfg(feature = "async-runtime")]
            UserEvent::DeferredMinimize { .. } => {
                unreachable!("DeferredMinimize перехватывается ранним возвратом выше")
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
            // ТЗ 0.14.3-fix8: при user close (X, WM_CLOSE) не сохраняем
            // geometry-файл — supervisor удалит оба файла. При swap
            // (LIGHT_SWAP=1) файл сохраняется для следующего child.
            self.closing = true;
            self.close_window(event_loop, id);
            return;
        }
        // Любое событие окна фиксирует его токен как активный/текущий. До
        // этого обновления происходили только на вводе (после проверки
        // блокировки), из-за чего между событиями другого окна (например,
        // RedrawRequested) глобальные токены оставались на предыдущем окне, и
        // post_update_to адресовал не тот компонент.
        set_active_window(token);
        set_current_window_token(token);
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
        match event {
            WindowEvent::Resized(size) => {
                window.invalidate();
                // ТЗ 0.15.1-fix3: вне restore-окна обновляем «желаемый»
                // размер (пользователь изменил размер окна). Если запись ещё
                // не существует (cold start, без restore) — создаём её из
                // текущего состояния окна.
                if !self.restoring_geometry {
                    let applied = self.last_applied_geometry.get_or_insert_with(|| {
                        let pos = window.window.outer_position().unwrap_or_default();
                        let sz = window.window.inner_size();
                        WindowGeometry {
                            x: pos.x,
                            y: pos.y,
                            width: sz.width,
                            height: sz.height,
                            maximized: false,
                            minimized: false,
                        }
                    });
                    applied.width = size.width;
                    applied.height = size.height;
                }
                self.flush_geometry(id);
            }
            WindowEvent::Moved(position) => {
                #[cfg(feature = "devtools")]
                {
                    eprintln!(
                        "[moved] pos=({}, {}), restoring={}",
                        position.x, position.y, self.restoring_geometry
                    );
                }
                // ТЗ 0.15.1-fix3: вне restore-окна обновляем «желаемую»
                // позицию на фактическую (пользователь перетащил окно). Если
                // запись ещё не существует (cold start, без restore) —
                // создаём её из текущего состояния окна.
                if !self.restoring_geometry {
                    let applied = self.last_applied_geometry.get_or_insert_with(|| {
                        let pos = window.window.outer_position().unwrap_or_default();
                        let sz = window.window.inner_size();
                        WindowGeometry {
                            x: pos.x,
                            y: pos.y,
                            width: sz.width,
                            height: sz.height,
                            maximized: false,
                            minimized: false,
                        }
                    });
                    applied.x = position.x;
                    applied.y = position.y;
                }
                self.flush_geometry(id);
            }
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
                // ТЗ 0.15.1-fix2: снимаем флаг восстановления геометрии через
                // 200 мс — к этому времени WM уже доставил все Moved-события
                // от нашего `set_outer_position`.
                if let Some(started) = self.restore_started_at
                    && started.elapsed() > std::time::Duration::from_millis(200)
                {
                    self.restoring_geometry = false;
                    self.restore_started_at = None;
                }
                if let Err(error) = window.redraw() {
                    self.error = Some(error);
                    event_loop.exit();
                }
            }
            _ => {}
        }
    }
}
