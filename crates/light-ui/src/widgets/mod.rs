//! Виджеты light-ui (0.21.0): Toast, Tooltip.
//!
//! Toast — многозадачный: `show_toast(text, duration_ms)` добавляет toast в
//! менеджер (`thread_local` [`toast::ToastManager`]), а `<toast-container>`
//! рендерит **все** активные toast'ы вертикальным столбиком. Tooltip —
//! кастомный элемент `<tooltip>` (прозрачный по умолчанию, видимость —
//! через `v-if` + `@mouseenter`/`@mouseleave` в шаблоне).
//!
//! Философия (0.21.0): фреймворк — **механизм**, пользователь — **политика**.
//! Виджеты прозрачны по умолчанию; цвета, шрифт и отступы задаёт пользователь
//! стандартными атрибутами стиля в HTML-шаблоне. Никаких «магических»
//! атрибутов, которых нет в `docs/guide.md`.

pub mod toast;
pub mod tooltip;

// Публичные реэкспорты: демо-приложения пишут
// `use light_ui::{show_toast, ToastContainer, Tooltip, ActiveToast};`.
pub use toast::{ActiveToast, ToastContainer, ToastManager, show_toast};
pub use tooltip::Tooltip;
