//! # Light UI (0.21.0)
//!
//! Готовые виджеты поверх `light-fw-core` + `light-gui`: Toast, Tooltip,
//! Modal и ProgressBar.
//!
//! * **Toast** — [`show_toast(text, duration_ms)`] добавляет toast в
//!   `thread_local` менеджер и запускает фоновую задачу автоскрытия
//!   (без блокировки UI); `<toast-container>` рендерит все активные
//!   toast'ы вертикальным столбиком. Позиция/цвет/отступы задаются
//!   стандартными атрибутами стиля в HTML.
//! * **Tooltip** — кастомный элемент `<tooltip text="…" position="absolute"
//!   top="…" left="…">`: прозрачный по умолчанию, стиль — из атрибутов
//!   тега, видимость полностью под контролем пользователя через `v-if`
//!   и `@mouseenter`/`@mouseleave` в шаблоне. Никакого внутреннего
//!   ховер-состояния и задержек.
//! * **Modal** — кастомный элемент `<modal title="…">контент</modal>`:
//!   модальное окно с заголовком, белым фоном, рамкой 2px и `z = 5000`
//!   (верхний слой); видимость — через `v-if` в шаблоне. По умолчанию —
//!   абсолютный оверлей (`position="absolute"`, отступ 50px от края).
//! * **ProgressBar** — кастомный элемент `<progress value="…" max="…"
//!   color="…" background="…" height="…">`: полоса заполнения **в потоке**
//!   (без `position`/`z`); ширина = `clamp(value, 0, max) / max * 100%`.
//!   За фичей `widget-progress` (в default).

pub mod widgets;

// Публичные реэкспорты: демо-приложения пишут
// `use light_ui::{show_toast, ActiveToast, ToastContainer, ToastManager, Tooltip, Modal};`.
pub use widgets::{ActiveToast, Modal, ToastContainer, ToastManager, Tooltip, show_toast};

#[cfg(feature = "widget-progress")]
pub use widgets::ProgressBar;
