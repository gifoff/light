//! Оконная обвязка поверх `winit` и сменного бэкенда рендеринга.
//!
//! Создаёт безрамочное окно (параметры берутся из [`Component::window_config`]),
//! рисует интерфейс программным растеризатором в RGBA-буфер, обрабатывает
//! ввод, перетаскивание за `<titlebar>` и команды управления окном
//! (`close()`, `minimize()`, `maximize()`, …).
//!
//! Бэкенд поддерживает несколько окон: главное окно и дочерние модальные
//! (`Component::modal`). Команда адресуется окну по [`WindowToken`], который
//! запоминается перед обработкой события.

mod a11y_bridge;
mod app;
mod cursor;
mod devtools;
mod events;
pub mod font;
mod geometry;
mod icon;
#[cfg(feature = "image-support")]
mod image;
mod interaction;
mod keyboard;
mod keys;
mod open_windows;
pub(crate) mod paint;
mod render_loop;
mod scrollbar;
mod state;
mod text;
mod windows;

pub use app::run;
// Курсоры: тесты light-core/tests/cursor.rs и публичный API
// `light_fw::window::{cursor_for, default_cursor_for, resolve_cursor}`.
pub use cursor::{cursor_for, default_cursor_for, resolve_cursor};
