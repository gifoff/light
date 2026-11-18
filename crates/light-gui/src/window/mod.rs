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
pub mod font;
mod icon;
mod interaction;
mod keys;
mod keyboard;
#[cfg(feature = "image-support")]
mod image;
pub(crate) mod paint;
mod render_loop;
mod scrollbar;
mod state;
mod text;
mod windows;
mod open_windows;
mod devtools;
mod events;
mod geometry;

pub use app::run;


