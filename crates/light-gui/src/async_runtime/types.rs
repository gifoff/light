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
//! Прокси устанавливается один раз в [`crate::window::run`] через
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

use std::any::Any;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::thread::{JoinHandle, Result as ThreadResult};

use thiserror::Error;
use winit::event_loop::EventLoopProxy;
use winit::window::WindowId;

#[cfg(feature = "devtools")]
use crate::devtools::DevToolsCommand;
// Этап 4: `Component` — из `light_vdom::component`; `WindowCommand`/
// `WindowToken` — из `light_vdom::window_backend` (канал + `WindowBackend`
// trait; core реэкспортирует их как `light_fw_core::control` для
// обратной совместимости 0.19.x).
use light_vdom::window_backend::{WindowCommand, WindowToken};

/// Типизированное действие над состоянием компонента в UI-потоке.
///
/// Принимает `&mut dyn Any` и делает downcast к целевому типу сам;
/// тип проверяется внутри [`post_update`], а не в [`UserEvent::Action`].
pub type UpdateFn = Box<dyn FnOnce(&mut dyn Any) + Send + 'static>;

/// События цикла событий окна.
///
/// Единственный тип пользовательских событий, который использует
/// [`crate::window`]: [`UserEvent::Command`] адресует команду конкретному
/// окну по [`WindowToken`], [`UserEvent::Access`] — событие
/// assistive-technology-клиента (только при фиче `a11y-winit`), а
/// [`UserEvent::UpdateUI`] — замыкание, выполняемое в UI-потоке с
/// последующей перерисовкой всех открытых окон.
pub enum UserEvent {
    /// Команда управления окном, адресованная по [`WindowToken`].
    Command {
        /// Токен окна-получателя.
        token: WindowToken,
        /// Команда, которую нужно применить к окну.
        command: WindowCommand,
    },
    /// Событие assistive-technology-клиента (только при фиче `a11y-winit`).
    #[cfg(feature = "a11y-winit")]
    Access(accesskit_winit::Event),
    /// Замыкание, поставленное в очередь через [`post_to_ui`].
    UpdateUI(Box<dyn FnOnce() + Send + 'static>),
    /// Команда инспектора DevTools (клик по элементу, изменение состояния).
    /// Применяется в UI-потоке к активному окну; результат клика возвращается
    /// по каналу внутри [`DevToolsCommand`] TCP-серверу.
    #[cfg(feature = "devtools")]
    DevTools(DevToolsCommand),
    /// Типизированное обновление компонента через [`post_update`]
    /// или [`post_update_to`].
    ///
    /// `f` — замыкание, которое выполняется в UI-потоке над
    /// `&mut dyn Any` и делает downcast к целевому типу `C` сам.
    ///
    /// Поле `target` хранит `TypeId` компонента, на который выполняется
    /// downcast в UI-потоке. Поле `token` задаёт точную адресацию окна:
    /// `None` — первое совпадение по `TypeId` (поведение [`post_update`]),
    /// `Some(t)` — только окно с этим токеном (поведение [`post_update_to`]);
    /// если целевое окно закрыто, действие отбрасывается с лог-сообщением.
    Action {
        /// Замыкание для обновления состояния компонента.
        f: UpdateFn,
        /// `TypeId` целевого типа компонента (задаётся в [`post_update`]).
        target: std::any::TypeId,
        /// Токен окна-цели для точной адресации (см. описание выше).
        token: Option<WindowToken>,
    },
    /// ТЗ 0.14.3-fix9: отложенный minimize — спавним окно видимым,
    /// чтобы первый frame отрисовался (taskbar-thumbnail), затем сворачиваем.
    DeferredMinimize {
        /// Идентификатор окна, которое нужно минимизировать.
        window_id: WindowId,
    },
}

impl std::fmt::Debug for UserEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Command { token, command } => f
                .debug_struct("Command")
                .field("token", token)
                .field("command", command)
                .finish(),
            #[cfg(feature = "a11y-winit")]
            Self::Access(event) => f.debug_tuple("Access").field(event).finish(),
            Self::UpdateUI(_) => f.debug_struct("UpdateUI").field("f", &"<closure>").finish(),
            #[cfg(feature = "devtools")]
            Self::DevTools(cmd) => f.debug_tuple("DevTools").field(cmd).finish(),
            Self::Action {
                f: _,
                target,
                token,
            } => f
                .debug_struct("Action")
                .field("f", &"<closure>")
                .field("target", target)
                .field("token", token)
                .finish(),
            Self::DeferredMinimize { window_id } => f
                .debug_struct("DeferredMinimize")
                .field("window_id", window_id)
                .finish(),
        }
    }
}

#[cfg(feature = "a11y-winit")]
impl From<accesskit_winit::Event> for UserEvent {
    fn from(event: accesskit_winit::Event) -> Self {
        Self::Access(event)
    }
}

/// Ошибка асинхронного обновления UI.
#[derive(Debug, Error)]
pub enum TaskError {
    /// UI-цикл ещё не запущен (`install_proxy` не был вызван).
    #[error("the event loop is not running")]
    NotRunning,

    /// UI-цикл уже завершён или прокси был удалён, событие не доставить.
    #[error("the event loop is closed")]
    Closed,

    /// Прокси уже установлен — `install_proxy` вызывается только один раз.
    #[error("the event loop proxy is already installed")]
    AlreadyInstalled,
}

/// Контекст, переданный замыканию [`spawn`] при запуске.
///
/// Позволяет задаче видеть запрос отмены: поток сам решает, где прерваться.
#[derive(Debug, Clone)]
pub struct TaskContext {
    pub(crate) cancel_flag: Arc<AtomicBool>,
}

impl TaskContext {
    /// Возвращает `true`, если на ручку [`TaskHandle::abort`] было вызвано
    /// (или планируется) отмена.
    pub fn is_cancelled(&self) -> bool {
        self.cancel_flag.load(Ordering::SeqCst)
    }
}

/// Ручка на поток, запущенный через [`spawn`].
///
/// Храни, если нужно дождаться завершения; просто сбрось, если задача
/// должна работать независимо от вызывающего кода.
#[must_use]
pub struct TaskHandle {
    pub(crate) inner: JoinHandle<()>,
    pub(crate) cancel_flag: Arc<AtomicBool>,
}

impl TaskHandle {
    /// Просит задачу остановиться: выставляет флаг, который задача читает
    /// через [`TaskContext::is_cancelled`]. Отмена кооперативная — поток
    /// сам решает, где прерваться.
    pub fn abort(&self) {
        self.cancel_flag.store(true, Ordering::SeqCst);
    }

    /// Ждёт завершения потока.
    ///
    /// Завёрнутый в поток замыкание не возвращает значений, поэтому
    /// результат — [`std::thread::Result`], который `Ok(())` при нормальном
    /// завершении и `Err` с паник-пейлоадем при падении.
    pub fn join(self) -> ThreadResult<()> {
        self.inner.join()
    }

    /// Возвращает флаг отмены, которым делится запущенный поток.
    ///
    /// Позволяет хранить сигнал отмены отдельно от ручки (например, в
    /// состоянии UI, где сама ручка с её [`JoinHandle`] не помещается),
    /// сохраняя возможность запросить отмену в любой момент.
    pub fn cancel_flag(&self) -> Arc<AtomicBool> {
        self.cancel_flag.clone()
    }
}

/// Прокси UI-цикла, установленный один раз в [`crate::window::run`].
pub(crate) static UI_PROXY: OnceLock<EventLoopProxy<UserEvent>> = OnceLock::new();

/// Устанавливает прокси UI-цикла (однократно).
///
/// Вызывается в [`crate::window::run`] сразу после создания `EventLoop`.
/// Повторный вызов возвращает [`TaskError::AlreadyInstalled`].
pub(crate) fn install_proxy(proxy: EventLoopProxy<UserEvent>) -> Result<(), TaskError> {
    UI_PROXY.set(proxy).map_err(|_| TaskError::AlreadyInstalled)
}
