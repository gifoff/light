//! Инспекция состояния приложения: JSON-снимок дерева, реактивных свойств
//! и (в перспективе) раскладки для инструментов разработки (DevTools).
//!
//! При включённой фиче `devtools`:
//! * [`capture_snapshot`] строит снимок текущего состояния компонента;
//! * [`update_latest_snapshot`] / [`read_latest_snapshot`] передают последний
//!   снимок (как JSON-строку) из UI-потока в TCP-сервер;
//! * [`send_command`] / [`drain_commands`] — канал управления: TCP-сервер
//!   отправляет команды (`Click`, `SetState`) в UI-поток, где они
//!   применяются к компоненту;
//! * модуль [`server`] — TCP-сервер на `127.0.0.1`, который отдаёт
//!   последний снимок по команде `SNAPSHOT\n` и принимает управляющие
//!   команды (`CLICK`, `SET_STATE`, `PING`).

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, OnceLock};

use crate::component::Component;

/// Команда управления, отправляемая из TCP-сервера в UI-поток.
#[derive(Debug, Clone)]
pub enum DevToolsCommand {
    /// Симулировать клик по элементу с ключом `key`
    /// (вызывает [`Component::dispatch`] с [`Event::Click`](crate::vdom::Event::Click)).
    Click(String),
    /// Изменить значение реактивного поля `field` на `value`.
    /// Пока заглушка: применяется, если поле поддерживает прямую запись.
    SetState {
        /// Имя поля.
        field: String,
        /// Новое значение (JSON).
        value: serde_json::Value,
    },
}

/// Глобальное хранение последнего JSON-снимка: обновляется из UI-потока при
/// каждой перерисовке, читается TCP-сервером на отдельном потоке.
static LATEST_SNAPSHOT: OnceLock<Arc<Mutex<Option<String>>>> = OnceLock::new();

/// Возвращает (и при необходимости создаёт) глобальное хранилище снимка.
fn latest_snapshot_slot() -> &'static Arc<Mutex<Option<String>>> {
    LATEST_SNAPSHOT.get_or_init(|| Arc::new(Mutex::new(None)))
}

/// Сохраняет последнюю JSON-строку снимка.
///
/// Вызывается из UI-потока (обработчик перерисовки) после каждого кадра.
/// Если `LATEST_SNAPSHOT` ещё не было создано, создаётся автоматически.
pub fn update_latest_snapshot(json: String) {
    let slot = latest_snapshot_slot();
    if let Ok(mut guard) = slot.lock() {
        *guard = Some(json);
    }
}

/// Читает последний JSON-снимок для TCP-сервера.
///
/// Возвращает клон строки или `None`, если снимок ещё не был захвачен.
/// Безопасно вызывать из любого потока.
#[doc(hidden)]
pub(crate) fn read_latest_snapshot() -> Option<String> {
    let slot = latest_snapshot_slot();
    slot.lock().ok()?.clone()
}

// ─── Канал управления: TCP → UI ──────────────────────────────────────────────

/// Глобальная очередь команд управления DevTools. TCP-сервер пушит команды,
/// UI-поток дренирует их при перерисовке. `Mutex` защищает от гонок;
/// критическая секция короткая (push/try_pop), contention не проблема для
/// dev-инструмента.
static COMMAND_QUEUE: OnceLock<Arc<Mutex<VecDeque<DevToolsCommand>>>> = OnceLock::new();

/// Возвращает (и при необходимости создаёт) глобальную очередь команд.
fn command_queue() -> &'static Arc<Mutex<VecDeque<DevToolsCommand>>> {
    COMMAND_QUEUE.get_or_init(|| Arc::new(Mutex::new(VecDeque::new())))
}

/// Отправляет команду управления в очередь для UI-потока.
///
/// Вызывается из TCP-сервера. Команда будет обработана при следующей
/// перерисовке; если UI ещё не запущено, она просто останется в очереди.
#[doc(hidden)]
pub(crate) fn send_command(cmd: DevToolsCommand) {
    let queue = command_queue();
    if let Ok(mut guard) = queue.lock() {
        guard.push_back(cmd);
    }
}

/// Извлекает все накопленные команды из очереди.
///
/// Вызывается из UI-потока при перерисовке. Команды обрабатываются
/// синхронно, до отрисовки следующего кадра.
#[doc(hidden)]
pub(crate) fn drain_commands() -> Vec<DevToolsCommand> {
    let queue = command_queue();
    let mut cmds = Vec::new();
    if let Ok(mut guard) = queue.lock() {
        while let Some(cmd) = guard.pop_front() {
            cmds.push(cmd);
        }
    }
    cmds
}

/// Снимок состояния интерфейса в формате JSON.
///
/// Передаётся инструментам разработки по WebSocket (задача 0.13-B) или
/// читается из памяти: содержит виртуальное дерево отрисовки, значения
/// реактивных свойств компонента и (опционально) раскладку.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DevToolsSnapshot {
    /// Токен окна, которому принадлежит снимок.
    pub window_token: u32,
    /// Виртуальное дерево ([`crate::vdom::VElement`]) в JSON-представлении.
    pub vdom: serde_json::Value,
    /// Значения реактивных свойств компонента (результат [`Component::inspect_state`]).
    pub state: serde_json::Value,
    /// Данные раскладки, если они доступны; в текущей версии — `None`.
    pub layout: Option<serde_json::Value>,
}

/// Компонент, состояние которого можно инспектировать в DevTools.
///
/// Реализуется автоматически для любого [`Component`] (через blanket impl);
/// конкретные значения реактивных свойств предоставляет реализация
/// [`Component::inspect_state`], генерируемая макросом `#[template]` (фича
/// `devtools`): каждое поле появляется под своим именем со значением геттера.
/// Поля с неподдерживаемым типом отображаются как `null`.
pub trait DevToolsInspect: Component {
    /// Снимок реактивных свойств компонента в формате JSON.
    fn inspect_state(&self) -> serde_json::Value;
}

// Blanket impl: любое `Component` уже имеет `inspect_state` (дефолтный
// возвращает `null`; `#[template]` его переопределяет при включённой фиче).
impl<T: Component + ?Sized> DevToolsInspect for T {
    fn inspect_state(&self) -> serde_json::Value {
        <Self as Component>::inspect_state(self)
    }
}

/// Снимает текущее состояние компонента: VDOM и реактивные свойства.
///
/// `token` — идентификатор окна, которому принадлежит снимок
/// (используется в DevTools для адресации). Раскладка пока не включается
/// (`layout = None`).
pub fn capture_snapshot(token: u32, component: &dyn Component) -> DevToolsSnapshot {
    DevToolsSnapshot {
        window_token: token,
        vdom: component.render().to_json(),
        state: component.inspect_state(),
        layout: None,
    }
}

/// TCP-сервер для DevTools: отдаёт последний JSON-снимок по построчному
/// протоколу (`SNAPSHOT\n` → JSON, `QUIT\n` → закрыть соединение).
pub mod server;
