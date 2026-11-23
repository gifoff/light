//! Виджеты light-ui (0.21.0): Toast, Tooltip.

pub mod toast;
pub mod tooltip;

// Публичные реэкспорты: демо-приложения пишут
// `use light_ui::{show_toast, show_toast_cfg, ToastConfig, Tooltip};`.
pub use toast::{ToastConfig, show_toast, show_toast_cfg};
pub use tooltip::Tooltip;
