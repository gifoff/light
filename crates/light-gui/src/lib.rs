//! # Light GUI (0.20.0, Этап 4)
//!
//! GUI-слой Light: окно на `winit`, события ввода, а11y-мост AccessKit,
//! рендер-бэкенды (`softbuffer`/`pixels`) и асинхронный рантайм
//! (фоновые задачи + доставка событий в UI-поток). Все эти модули раньше
//! жили в `light-fw-core`; в Этапе 4 они вынесены сюда, чтобы ядро
//! оставалось headless-собираемым без `winit`/`softbuffer`.
//!
//! `light-fw-core` реэкспортирует каждый из этих модулей под прежними именами
//! (`light_fw_core::window` и т.д.) для обратной совместимости с 0.19.x,
//! поэтому публичный API приложения не меняется.

pub mod a11y;

// Асинхронный рантайм. При выключенной фиче `async-runtime` экспортируется
// заглушка с теми же публичными именами, что и настоящий модуль
// (`post_to_ui`/`post_update`/`post_update_to` возвращают ошибку, потому
// что UI-потока нет; `TaskContext`/`TaskHandle`/`UserEvent` — маркеры),
// чтобы `light-fw-core` мог реэкспортировать их в headless-режиме.
// Публичный `stub` нужен, чтобы core мог re-экспортировать эти имена
// из `light_gui::stub` без привязки к конкретной фиче.
#[cfg(not(feature = "async-runtime"))]
pub mod stub {
    //! Заглушка асинхронного рантайма без `async-runtime`.
    use std::future::Future;

    /// Контекст задачи (заглушка).
    pub struct TaskContext;
    /// Ручка задачи (заглушка).
    pub struct TaskHandle;
    /// Ошибка запуска задачи (заглушка; не может быть порождена).
    pub enum TaskError {
        /// Заглушка: никогда не создаётся.
        Never,
    }
    /// Пользовательское событие UI-потока (заглушка; никогда не создаётся).
    pub enum UserEvent {
        /// Заглушка: никогда не создаётся.
        Never,
    }
    /// Отправка замыкания в UI-поток (заглушка: всегда ошибка).
    pub fn post_to_ui<F>(f: F) -> Result<(), TaskError>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        // Явно дропаем без `await`: фьючер не выполняется, мы только
        // проверяем сигнатуру в headless-режиме.
        std::mem::drop(f);
        Err(TaskError::Never)
    }
    /// Обновление UI (заглушка: всегда ошибка).
    pub fn post_update<C>(f: impl FnOnce(&mut C)) -> Result<(), TaskError> {
        let _ = f;
        Err(TaskError::Never)
    }
    /// Обновление UI в конкретном окне (заглушка: всегда ошибка).
    pub fn post_update_to<C, I>(_token: I, f: impl FnOnce(&mut C)) -> Result<(), TaskError> {
        let _ = f;
        Err(TaskError::Never)
    }
}

/// Настоящий асинхронный рантайм (включается фичей `async-runtime`).
#[cfg(feature = "async-runtime")]
pub mod async_runtime;
#[cfg(feature = "devtools")]
pub mod devtools;
pub mod error;
#[cfg(feature = "file-dialog")]
pub mod file_dialog;
// Рендер-бэкенды (`softbuffer`/`pixels`) и окно на `winit`: существуют только
// при фиче `window` (иначе нет ни winit, ни softbuffer).
#[cfg(feature = "window")]
pub mod render;
#[cfg(feature = "window")]
pub mod window;

// Этап 4: все `crate::`-пути к vdom/layout/component/control в перенесённом
// коде переписаны на прямые пути `light_vdom::...` (точечно, скриптами в
// target/temp/). Канал оконных операций (`WindowCommand`/`WindowToken`/
// `set_*`) живёт в `light_vdom::window_backend` с этапа 3; light-gui —
// один из потребителей, а `light-fw-core` реэкспортирует его как
// `light_fw_core::control` для обратной совместимости 0.19.x. Обратной
// зависимости `light-fw-core → light-gui` нет (только `light-gui →
// light-vdom` и `light-fw-core → light-gui`), циклов не возникает:
// headless-сборка core без фич не тянет winit/softbuffer/pixels/rfd.

pub use error::{Error, Result};
