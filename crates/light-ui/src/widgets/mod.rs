//! Виджеты light-ui (0.21.0): Toast, Tooltip.

pub mod toast;
pub mod tooltip;

// Публичные реэкспорты: демо-приложения пишут `use light_ui::{show_toast, Tooltip};`.
pub use toast::show_toast;
pub use tooltip::Tooltip;
