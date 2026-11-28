//! Виджеты light-fw-ui (0.21.0): Toast, Tooltip, Dialog, ProgressBar.
//!
//! Toast — многозадачный: `show_toast(text, duration_ms)` добавляет toast в
//! менеджер (`thread_local` [`toast::ToastManager`]), а `<toast-container>`
//! рендерит **все** активные toast'ы вертикальным столбиком. Tooltip —
//! кастомный элемент `<tooltip>` (прозрачный по умолчанию, видимость —
//! через `v-if` + `@mouseenter`/`@mouseleave` в шаблоне). Dialog —
//! кастомный элемент `<dialog title="…">контент</dialog>`: диалоговое окно с
//! заголовком, белым фоном, рамкой 2px и `z = 5000` (верхний слой);
//! видимость — через `v-if`. ProgressBar — кастомный элемент
//! `<progress value="…" max="…" color="…" background="…" height="…">`:
//! полоса заполнения в **потоке** (без `position`/`z`), ширина =
//! `clamp(value, 0, max) / max * 100%`; за фичей `widget-progress`.
//!
//! Философия (0.21.0): фреймворк — **механизм**, пользователь — **политика**.
//! Виджеты прозрачны по умолчанию; цвета, шрифт и отступы задаёт пользователь
//! стандартными атрибутами стиля в HTML-шаблоне. Никаких «магических»
//! атрибутов, которых нет в `docs/guide.md`.

pub(crate) mod common;
pub mod dialog;
pub mod toast;
pub mod tooltip;

#[cfg(feature = "widget-progress")]
pub mod progress_bar;

// Публичные реэкспорты: демо-приложения пишут
// `use light_fw_ui::{show_toast, ToastContainer, Tooltip, ActiveToast, Dialog};`.
pub use dialog::Dialog;
pub use toast::{ActiveToast, ToastContainer, ToastManager, show_toast};
pub use tooltip::Tooltip;

#[cfg(feature = "widget-progress")]
pub use progress_bar::ProgressBar;
