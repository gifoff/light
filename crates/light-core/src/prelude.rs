//! Перечень того, что принято подключать в прикладном коде: `use light_fw::prelude::*;`.

pub use crate::component::{Component, WindowConfig};
pub use crate::dom::Dom;
#[cfg(feature = "image-support")]
pub use crate::vdom::ImageData;
pub use crate::vdom::{
    Cursor, Edges, Event, KeyInfo, MouseButton, NodeKind, Overflow, Position, SelectOption,
    VElement, VNode, VWindow, WhiteSpace,
};
pub use crate::{proxy, template};
pub use light_reactivity::Collection;
pub use light_reactivity::{Property, Proxy, ProxyState, Watcher, watch};

/// Доступность: роли AccessKit и идентификаторы узлов.
///
/// Этап 4 (Strangler Fig): тип `accesskit` живёт в `light-gui` — там, где
/// реально строится дерево AccessKit; ядро пробрасывает зависимости фичи
/// `a11y`, но не берёт `accesskit` как прямую зависимость. Реэкспорт
/// под прежним именем для обратной совместимости с 0.19.x:
/// `NodeId`/`AriaRole` реэкспортируются из `light_gui::a11y`.
#[cfg(feature = "a11y")]
pub use light_gui::a11y::{AriaRole, NodeId};

// GUI-модуль a11y перебазиан в `light-gui` (этап 4) — реэкспортируем.
pub use light_gui::a11y;

// Асинхронный рантайм перебазиан в `light-gui` (этап 4); без фичи — заглушка
// с теми же именами (см. `crate::async_runtime` в `lib.rs`).
#[cfg(feature = "async-runtime")]
pub use light_gui::async_runtime::{post_update, post_update_to};
/// Асинхронное обновление UI: `post_update` и `Any` для downcast-а в UI-потоке.
#[cfg(feature = "async-runtime")]
pub use std::any::Any;
