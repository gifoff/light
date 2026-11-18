//! # Light VDOM — виртуальный DOM, раскладка и runtime (этап 3)
//!
//! Этот крейт содержит слои **VDOM** (виртуальное дерево интерфейса, diff,
//! обработка событий), **layout** (раскладка элементов), **DOM** (монтаж/
//! димонтаж в живое дерево), **runtime** (парсинг HTML-шаблонов во время
//! выполнения), **plugins** (Plugin API для кастомных элементов),
//! **validation** (валидация форм) и **error** (типы ошибок VDOM-слоя).
//!
//! Перенесено из `light-fw-core` на этапе 3 декомпозиции (Strangler Fig).
//! `light-fw-core` реэкспортирует все эти модули под прежними именами для
//! обратной совместимости с 0.19.x.
//!
//! ## Зависимости
//!
//! `light-vdom` **не зависит** от `light-fw-core` (это было бы циклической
//! зависимостью). Оконные операции (`close`, `minimize`, …) идут через канал
//! [`window_backend`], который делегирует в `light-fw-core::control` при
//! включённой фиче `window`.

#![deny(missing_docs)]

pub mod component;
pub mod dom;
pub mod error;
pub mod layout;
pub mod plugins;
pub mod runtime;
pub mod text_wrap;
pub mod validation;
pub mod vdom;
pub mod window_backend;

// Реэкспорт основных типов для удобства.
pub use component::{Component, WindowConfig};
pub use dom::Dom;
pub use error::{Error, Result};
#[cfg(feature = "image-support")]
pub use vdom::ImageData;
#[cfg(feature = "window")]
pub use vdom::VWindow;
pub use vdom::{
    Align, Edges, Event, KeyInfo, MouseButton, NodeKind, Offset, Overflow, Position, Resize,
    SelectOption, Size, Style, VElement, VNode, WhiteSpace,
};
