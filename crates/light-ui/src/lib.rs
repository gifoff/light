//! # Light UI (0.21.0)
//!
//! Готовые виджеты поверх `light-fw-core` + `light-gui`: Tooltip, Toast,
//! Modal, Progress. В этой версии — `Tooltip` (кастомный элемент `<tooltip>`
//! через `linkme`, реакция на `Mouseenter`/`Mouseleave`) и `Toast`
//! (`show_toast` с автоскрытием через 3 секунды, без блокировки UI).

pub mod widgets;

// Публичные реэкспорты: демо-приложения пишут
// `use light_ui::{show_toast, show_toast_cfg, ToastConfig, Tooltip};`.
pub use widgets::{ToastConfig, Tooltip, show_toast, show_toast_cfg};
