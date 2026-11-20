use super::app::App;
#[allow(unused_imports)]
use light_vdom::{Event as DomEvent, KeyInfo};

impl App {
    #[cfg(feature = "a11y-winit")]
    pub(super) fn on_access_event(&mut self, event: accesskit_winit::Event) {
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

    /// Применяет команду инспектора DevTools ([`UserEvent::DevTools`]) к
    /// активному окну.
    ///
    /// Клик по элементу (`Click`) вызывается через [`Component::dispatch`];
    /// изменение состояния (`SetState`) — через
    /// [`Component::inject_state`](light_vdom::component::Component::inject_state),
    /// восстановление значений из `.light/state.json` (команда `SAVE_STATE`)
    /// пишет файл состояния в UI-потоке. Результат обработки (`Click`/
    /// `SetState`) возвращается TCP-серверу по каналу `response_tx`; при
    /// обработке компонент помечается на перерисовку, а новый снимок
    /// обновляет `LATEST_SNAPSHOT`, чтобы инспектор сразу увидел изменение.
    #[cfg(all(feature = "devtools", feature = "async-runtime"))]
    pub(super) fn handle_devtools_command(&mut self, cmd: crate::devtools::DevToolsCommand) {
        match cmd {
            crate::devtools::DevToolsCommand::Click { key, response_tx } => {
                // Активное окно — то, к которому адресуются команды; если его
                // нет, клик некуда применить и отвечаем «не обработан».
                let Some(id) = self
                    .tokens
                    .get(&light_vdom::window_backend::active_window())
                    .copied()
                else {
                    let _ = response_tx.send(false);
                    return;
                };
                let Some(window) = self.windows.get_mut(&id) else {
                    let _ = response_tx.send(false);
                    return;
                };
                // 1. Статическая/макросная диспетчеризация: ветки `dispatch`
                //    (явные `key="..."` и позиционные ключи из compile-time).
                // 2. Fallback: прямое дерево по `vnode.key == key` — работает
                //    для авто-позиционных ключей (`.0.1`) при runtime-шаблоне,
                //    где позиции могли сдвинуться после hot-reload.
                let event = DomEvent::Click;
                let handled = window.component.dispatch(&key, event.clone())
                    || window.component.dispatch_vmodel(&key, &event);
                if handled {
                    window.invalidate();
                }
                let _ = response_tx.send(handled);
            }
            // SetState: применяем новое значение поля к компоненту активного
            // окна. Состояние передано парой `field`/`value` (см. сервер),
            // но `inject_state` принимает объект `{field: value}` — собираем
            // его здесь. Если поля нет или десериализация не удалась (тип не
            // совпадает), `inject_state` ничего не меняет — считаем команду
            // применённой (активное окно найдено, перерисовка запрошена).
            crate::devtools::DevToolsCommand::SetState {
                field,
                value,
                response_tx,
            } => {
                let Some(id) = self
                    .tokens
                    .get(&light_vdom::window_backend::active_window())
                    .copied()
                else {
                    let _ = response_tx.send(false);
                    return;
                };
                let Some(window) = self.windows.get_mut(&id) else {
                    let _ = response_tx.send(false);
                    return;
                };
                let mut state = serde_json::Map::new();
                state.insert(field, value);
                window
                    .component
                    .inject_state(&serde_json::Value::Object(state));
                window.invalidate();
                let _ = response_tx.send(true);
            }
            // SaveState: пишем текущее состояние компонента активного окна в
            // файл `.light/state.json`. Ошибки записи логируем в stderr — UI
            // продолжает работать; инспектор уже получил ответ `ok`.
            crate::devtools::DevToolsCommand::SaveState => {
                // Ищем окно по активному токену; если не найдено (stale token
                // в dev-mode), фолбэкаем на первое корневое окно (parent = None).
                let active_wid = self
                    .tokens
                    .get(&light_vdom::window_backend::active_window())
                    .copied()
                    .filter(|wid| self.windows.contains_key(wid));
                let root_wid = active_wid.or_else(|| {
                    self.windows
                        .iter()
                        .find(|(_, w)| w.parent.is_none())
                        .map(|(wid, _)| *wid)
                });
                let Some((root_wid, window)) =
                    root_wid.and_then(|wid| self.windows.get(&wid).map(|w| (wid, w)))
                else {
                    return;
                };
                eprintln!("light: SaveState → window_id={root_wid:?}");
                if let Err(error) =
                    crate::devtools::save_state_file(&window.component.inspect_state())
                {
                    eprintln!("DevTools: не удалось сохранить состояние: {error}");
                }
            }
            // ТЗ 0.15.1-fix3: graceful save перед swap-kill. Supervisor
            // шлёт SAVE_ALL по TCP; мы синхронно пишем state.json +
            // принудительно window.json (без дебаунса), затем отвечаем ok.
            crate::devtools::DevToolsCommand::SaveAll { response_tx } => {
                let active_wid = self
                    .tokens
                    .get(&light_vdom::window_backend::active_window())
                    .copied()
                    .filter(|wid| self.windows.contains_key(wid));
                let root_wid = active_wid.or_else(|| {
                    self.windows
                        .iter()
                        .find(|(_, w)| w.parent.is_none())
                        .map(|(wid, _)| *wid)
                });
                let Some((root_wid, window)) =
                    root_wid.and_then(|wid| self.windows.get(&wid).map(|w| (wid, w)))
                else {
                    let _ = response_tx.send(false);
                    return;
                };
                // 1) state.json
                let mut ok = true;
                if let Err(error) =
                    crate::devtools::save_state_file(&window.component.inspect_state())
                {
                    eprintln!("light: SAVE_ALL: не удалось сохранить state.json: {error}");
                    ok = false;
                }
                // 2) window.json — принудительная запись без дебаунса
                self.flush_geometry_force(root_wid);
                eprintln!("[SAVE_ALL] sent by supervisor, replied ok");
                let _ = response_tx.send(ok);
            }
            // ReloadTemplate: перечитываем HTML-шаблон с диска (runtime-templates)
            // и перерисовываем все окна. Кэш шаблонов ключируется по mtime,
            // поэтому при изменённом файле `render_dynamic` вернёт новый
            // VElement; при неизменённом — кэшированный (дешёвый).
            #[cfg(feature = "runtime-templates")]
            crate::devtools::DevToolsCommand::ReloadTemplate => {
                for window in self.windows.values_mut() {
                    window.invalidate();
                }
            }
            #[cfg(not(feature = "runtime-templates"))]
            crate::devtools::DevToolsCommand::ReloadTemplate => {
                // Фича runtime-templates не включена — команда без эффекта.
            }
            // ─── ТЗ 0.16.0: агентовые команды (TREE / LAYOUT / FIND / TYPE /
            // KEY / SHOT) ──────────────────────────────────────────────────
            // Все работают с активным окном (как Click/SetState): резолвим его
            // по токену, при отсутствии отвечаем ошибкой.
            #[allow(clippy::too_many_lines)]
            crate::devtools::DevToolsCommand::Type {
                key,
                text,
                response_tx,
            } => {
                let Some(id) = self
                    .tokens
                    .get(&light_vdom::window_backend::active_window())
                    .copied()
                else {
                    let _ = response_tx.send(Some("error: no active window".to_owned()));
                    return;
                };
                let Some(window) = self.windows.get_mut(&id) else {
                    let _ = response_tx.send(Some("error: no active window".to_owned()));
                    return;
                };
                // Узел должен быть текстовым полем (не отключённым).
                let is_textfield = window
                    .layout
                    .as_ref()
                    .and_then(|l| l.node(&key))
                    .is_some_and(|n| {
                        matches!(
                            &n.kind,
                            light_vdom::NodeKind::TextField {
                                disabled: false,
                                ..
                            }
                        )
                    });
                if !is_textfield {
                    let _ = response_tx
                        .send(Some("error: key not found or not a text field".to_owned()));
                    return;
                }
                // Фокусируем поле, заменяем содержимое, применяем через
                // commit_text (диспатчит TextChange и invalidates).
                let key_owned = key.clone();
                let before = window.editing.clone();
                window.snapshot_before_edit(&before);
                window.focused = Some(key_owned);
                window.editing = text.clone();
                window.caret = text.chars().count();
                window.anchor = None;
                window.commit_text();
                let _ = response_tx.send(Some("ok".to_owned()));
            }
            crate::devtools::DevToolsCommand::Key {
                key,
                keyname,
                response_tx,
            } => {
                let Some(id) = self
                    .tokens
                    .get(&light_vdom::window_backend::active_window())
                    .copied()
                else {
                    let _ = response_tx.send(Some("error: no active window".to_owned()));
                    return;
                };
                let Some(window) = self.windows.get_mut(&id) else {
                    let _ = response_tx.send(Some("error: no active window".to_owned()));
                    return;
                };
                // Разбираем имя ключа. winit::KeyEvent в 0.30 имеет приватные
                // поля и не конструируется извне, поэтому семантику нажатия
                // воспроизводим напрямую (без построения KeyEvent).
                use winit::keyboard::NamedKey;
                let named = match keyname.as_str() {
                    "Enter" => NamedKey::Enter,
                    "Tab" => NamedKey::Tab,
                    "Escape" => NamedKey::Escape,
                    _ => {
                        let _ = response_tx.send(Some(format!("error: invalid key {keyname}")));
                        return;
                    }
                };
                // Фокус переключаем на указанный узел, если он существует.
                if window.layout.as_ref().and_then(|l| l.node(&key)).is_some() {
                    window.focused = Some(key.clone());
                }
                let focused_key = window.focused.clone().unwrap_or_default();
                // Enter: submit (если одиночная строка + кнопка submit), иначе
                // диспатчим Keydown/KeyUp.
                if named == NamedKey::Enter
                    && !window.modifiers.shift_key()
                    && !window.modifiers.control_key()
                    && !window.modifiers.alt_key()
                    && let Some(tree) = window.tree.clone()
                {
                    let single_line =
                        tree.get(&focused_key)
                            .map(|node| &node.kind)
                            .is_some_and(|k| {
                                matches!(
                                    k,
                                    light_vdom::NodeKind::TextField {
                                        multiline: false,
                                        disabled: false,
                                        ..
                                    }
                                )
                            });
                    if single_line
                        && let Some(button) =
                            light_vdom::validation::find_submit_button(&tree, &focused_key)
                    {
                        let button_key = button.key.to_owned();
                        window.dispatch_checked(&button_key, DomEvent::Click);
                        window.invalidate();
                        let _ = response_tx.send(Some("ok".to_owned()));
                        return;
                    }
                }
                // Escape: закрыть открытое select (если фича есть).
                if named == NamedKey::Escape {
                    #[cfg(feature = "elements-select")]
                    if window.open_select.is_some() {
                        window.open_select = None;
                        window.select_hover = None;
                        window.invalidate();
                        let _ = response_tx.send(Some("ok".to_owned()));
                        return;
                    }
                }
                // Общий путь: диспатчим Keydown + KeyUp в компонент.
                // `KeyInfo.key` — строковое представление именованного ключа
                // (`format!("{named:?}"`), как в `key_info`). Модификаторы
                // берём из `window.modifiers`.
                let mods = window.modifiers;
                let key_str = format!("{named:?}");
                let down = DomEvent::Keydown(KeyInfo::new(
                    key_str.clone(),
                    mods.shift_key(),
                    mods.control_key(),
                    mods.alt_key(),
                ));
                window.dispatch(&focused_key, down);
                let up = DomEvent::Keyup(KeyInfo::new(
                    key_str,
                    mods.shift_key(),
                    mods.control_key(),
                    mods.alt_key(),
                ));
                window.dispatch(&focused_key, up);
                window.invalidate();
                let _ = response_tx.send(Some("ok".to_owned()));
            }
            crate::devtools::DevToolsCommand::Tree { response_tx } => {
                let id = self
                    .tokens
                    .get(&light_vdom::window_backend::active_window())
                    .copied()
                    .filter(|wid| self.windows.contains_key(wid))
                    .or_else(|| {
                        self.windows
                            .iter()
                            .find(|(_, w)| w.parent.is_none())
                            .map(|(wid, _)| *wid)
                    });
                let Some(window) = id.and_then(|id| self.windows.get(&id)) else {
                    let _ = response_tx.send(Some("null".to_owned()));
                    return;
                };
                let json = window
                    .tree
                    .as_deref()
                    .map(crate::devtools::serialize_tree)
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "null".to_owned());
                let _ = response_tx.send(Some(json));
            }
            crate::devtools::DevToolsCommand::LayoutCmd { response_tx } => {
                let id = self
                    .tokens
                    .get(&light_vdom::window_backend::active_window())
                    .copied()
                    .filter(|wid| self.windows.contains_key(wid))
                    .or_else(|| {
                        self.windows
                            .iter()
                            .find(|(_, w)| w.parent.is_none())
                            .map(|(wid, _)| *wid)
                    });
                let Some(window) = id.and_then(|id| self.windows.get(&id)) else {
                    let _ = response_tx.send(Some("null".to_owned()));
                    return;
                };
                let json = match (&window.layout, window.tree.as_deref()) {
                    (Some(layout), tree) => {
                        crate::devtools::serialize_layout(layout, tree).to_string()
                    }
                    _ => "null".to_owned(),
                };
                let _ = response_tx.send(Some(json));
            }
            crate::devtools::DevToolsCommand::Find { query, response_tx } => {
                let id = self
                    .tokens
                    .get(&light_vdom::window_backend::active_window())
                    .copied()
                    .filter(|wid| self.windows.contains_key(wid))
                    .or_else(|| {
                        self.windows
                            .iter()
                            .find(|(_, w)| w.parent.is_none())
                            .map(|(wid, _)| *wid)
                    });
                let Some(window) = id.and_then(|id| self.windows.get(&id)) else {
                    let _ = response_tx.send(Some("null".to_owned()));
                    return;
                };
                let result = match &window.tree {
                    Some(tree) => {
                        crate::devtools::find_element(tree.as_ref(), window.layout.as_ref(), &query)
                    }
                    None => None,
                };
                let json = result
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "null".to_owned());
                let _ = response_tx.send(Some(json));
            }
            crate::devtools::DevToolsCommand::Shot { response_tx } => {
                let id = self
                    .tokens
                    .get(&light_vdom::window_backend::active_window())
                    .copied()
                    .filter(|wid| self.windows.contains_key(wid))
                    .or_else(|| {
                        self.windows
                            .iter()
                            .find(|(_, w)| w.parent.is_none())
                            .map(|(wid, _)| *wid)
                    });
                let Some(window) = id.and_then(|id| self.windows.get_mut(&id)) else {
                    let _ = response_tx.send(Some("error: no window".to_owned()));
                    return;
                };
                let (w, h) = window.surface.size();
                let buf: Vec<u8> = window.surface.frame_mut().to_vec();
                let b64 = crate::devtools::encode_screenshot(&buf, w, h);
                let _ = response_tx.send(Some(b64));
            }
        }
    }
}
