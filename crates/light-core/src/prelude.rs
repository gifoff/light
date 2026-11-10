//! Перечень того, что принято подключать в прикладном коде: `use light_fw::prelude::*;`.

pub use crate::collection::Collection;
pub use crate::component::{Component, WindowConfig};
pub use crate::dom::Dom;
pub use crate::reactive::{Property, Proxy, ProxyState, Watcher, watch};
#[cfg(feature = "image-support")]
pub use crate::vdom::ImageData;
pub use crate::vdom::{
    Cursor, Edges, Event, KeyInfo, MouseButton, NodeKind, Overflow, Position, SelectOption,
    VElement, VNode, VWindow, WhiteSpace,
};
pub use crate::{proxy, template};

/// Доступность: роли AccessKit и идентификаторы узлов.
#[cfg(feature = "a11y")]
pub use accesskit::{NodeId, Role as AriaRole};

pub use crate::a11y;

#[cfg(feature = "async-runtime")]
pub use crate::async_runtime::{post_update, post_update_to};
/// Асинхронное обновление UI: `post_update` и `Any` для downcast-а в UI-потоке.
#[cfg(feature = "async-runtime")]
pub use std::any::Any;
