//! Инспекция состояния приложения: JSON-снимок дерева, реактивных свойств
//! и (в перспективе) раскладки для инструментов разработки (DevTools).
//!
//! При включённой фиче `devtools`:
//! * [`capture_snapshot`] строит снимок текущего состояния компонента;
//! * [`update_latest_snapshot`] / [`read_latest_snapshot`] передают последний
//!   снимок (как JSON-строку) из UI-потока в TCP-сервер;
//! * модуль [`server`] — TCP-сервер на `127.0.0.1`, который отдаёт
//!   последний снимок по команде `SNAPSHOT\n`.

use std::sync::{Arc, Mutex, OnceLock};

use crate::component::Component;

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
