//! Инспекция состояния приложения: JSON-снимок дерева, реактивных свойств
//! и (в перспективе) раскладки для инструментов разработки (DevTools).

use crate::component::Component;

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
