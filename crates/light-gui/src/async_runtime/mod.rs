//! Опциональная поддержка асинхронного обновления UI (фича `async-runtime`).
//!
//! Тип [`UserEvent`] является единым типом пользовательских событий цикла
//! событий окна: помимо [`UserEvent::UpdateUI`] (закрытие, выполняемое
//! в UI-потоке с последующей перерисовкой всех открытых окон), он несёт
//! [`UserEvent::Command`] и (при фиче `a11y-winit`) [`UserEvent::Access`],
//! которыми обменивается [`crate::window`].
//!
//! [`spawn`] запускает заданное замыкание в отдельном потоке, [`post_to_ui`]
//! возвращает замыкание в цикл событий UI-потока, где оно выполняется,
//! и после выполнения на всех окнах запрашивается перерисовка.
//!
//! Прокси устанавливается один раз в [`crate::window::run_with_backend`] через
//! [`install_proxy`] и живёт в статике [`UI_PROXY`].
//!
//! ```rust
//! # #[cfg(feature = "window")]
//! # fn example() {
//! use light_gui::async_runtime::{post_to_ui, spawn_async, spawn};
//!
//! spawn(|ctx| {
//!     std::thread::sleep(std::time::Duration::from_secs(1));
//!     if !ctx.is_cancelled() {
//!         post_to_ui(|| { /* обновить состояние UI */ }).ok();
//!     }
//! });
//! # }
//! ```

/// Потоки и доставка: [`spawn`], [`spawn_async`], [`post_to_ui`],
/// [`post_update`], [`post_update_to`], [`apply_action`].
pub mod tasks;
#[cfg(test)]
mod tests;
/// Типы данных: [`UserEvent`], [`TaskError`], [`TaskContext`], [`TaskHandle`].
pub mod types;

pub use tasks::*;
pub use types::*;
