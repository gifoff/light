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
//! use light_fw_core::async_runtime::{post_to_ui, spawn};
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

use crate::{WindowCommand, component::Component, control::WindowToken};
#[cfg(feature = "a11y-winit")]
use accesskit_winit;

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
    cancel_flag: Arc<AtomicBool>,
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
    inner: JoinHandle<()>,
    cancel_flag: Arc<AtomicBool>,
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

/// Запускает замыкание в отдельном потоке.
///
/// Замыкание получает [`TaskContext`], через который замечает запрос отмены
/// (кооперативно — поток сам решает, где прерваться). Поток живёт
/// независимо от вызывающего кода: [`TaskHandle`] можно отбросить, и задача
/// продолжит выполнение.
pub fn spawn<F>(f: F) -> TaskHandle
where
    F: FnOnce(TaskContext) + Send + 'static,
{
    let cancel_flag = Arc::new(AtomicBool::new(false));
    let ctx = TaskContext {
        cancel_flag: cancel_flag.clone(),
    };
    let inner = std::thread::Builder::new()
        .name("light-async".into())
        .spawn(move || f(ctx))
        .expect("thread spawn should not fail");
    TaskHandle { inner, cancel_flag }
}

/// Ставит замыкание в очередь UI-цикла.
///
/// Замыкание выполнится в UI-потоке, после выполнения на всех открытых
/// окнах запрашивается перерисовка.
///
/// # Ошибки
///
/// * [`TaskError::NotRunning`] — если `install_proxy` ещё не был вызван
///   (UI-цикл не запущен).
/// * [`TaskError::Closed`] — если UI-цикл уже завершён.
pub fn post_to_ui<F>(f: F) -> Result<(), TaskError>
where
    F: FnOnce() + Send + 'static,
{
    let proxy = match UI_PROXY.get() {
        Some(proxy) => proxy,
        None => return Err(TaskError::NotRunning),
    };
    proxy
        .send_event(UserEvent::UpdateUI(Box::new(f)))
        .map_err(|_| TaskError::Closed)
}

/// Ставит типизированное обновление компонента в очередь UI-цикла.
///
/// Замыкание выполнится в UI-потоке, где компонент будет обновлён,
/// и на всех открытых окнах запрашивается перерисовка.
///
/// В отличие от [`post_to_ui`], замыкание получает ссылку на конкретный
/// тип компонента, что позволяет обновлять `!Send`-состояние компонента,
/// не нарушая границы `Send` при пересылке в UI-поток.
///
/// # Ошибки
///
/// * [`TaskError::NotRunning`] — если `install_proxy` ещё не был вызван
///   (UI-цикл не запущен).
/// * [`TaskError::Closed`] — если UI-цикл уже завершён.
pub fn post_update<C: Component + 'static>(
    f: impl FnOnce(&mut C) + Send + 'static,
) -> Result<(), TaskError> {
    post_update_impl(None, f)
}

/// Общий путь для [`post_update`] и [`post_update_to`]: упаковывает
/// замыкание в [`UserEvent::Action`] и отправляет его в очередь UI-цикла.
///
/// * `token == None` — адресация по `TypeId` (первое окно с компонентом `C`);
/// * `token == Some(t)` — адресация к окну с токеном `t`.
fn post_update_impl<C: Component + 'static>(
    token: Option<WindowToken>,
    f: impl FnOnce(&mut C) + Send + 'static,
) -> Result<(), TaskError> {
    let proxy = match UI_PROXY.get() {
        Some(proxy) => proxy,
        None => return Err(TaskError::NotRunning),
    };
    let action = UserEvent::Action {
        f: Box::new(move |any: &mut dyn Any| {
            if let Some(component) = any.downcast_mut::<C>() {
                f(component);
            }
        }),
        target: std::any::TypeId::of::<C>(),
        token,
    };
    proxy.send_event(action).map_err(|_| TaskError::Closed)
}

/// Ставит типизированное обновление компонента в очередь UI-цикла
/// с адресацией к конкретному окну по его [`WindowToken`].
///
/// Замыкание выполнится в UI-потоке и применится **только** к окну,
/// чей токен совпадает с `token`. Если окно с этим токеном уже закрыто,
/// обновление тихо отбрасывается (в UI-потоке логируется
/// `async-runtime: target window closed`).
///
/// Токен окна получается через [`crate::control::current_window_token`]
/// (из UI-потока, например из `on_click`) или хранится в состоянии
/// компонента (поле типа `WindowToken` в `#[proxy]`-структуре).
///
/// # Ошибки
///
/// * [`TaskError::NotRunning`] — если `install_proxy` ещё не был вызван
///   (UI-цикл не запущен).
/// * [`TaskError::Closed`] — если UI-цикл уже завершён.
pub fn post_update_to<C: Component + 'static>(
    token: WindowToken,
    f: impl FnOnce(&mut C) + Send + 'static,
) -> Result<(), TaskError> {
    post_update_impl(Some(token), f)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    #[test]
    fn post_to_ui_without_install_errors() {
        let result = post_to_ui(|| {});
        match result {
            Err(TaskError::NotRunning) => {}
            other => panic!("expected NotRunning, got {other:?}"),
        }
    }

    #[test]
    fn spawn_runs_closure() {
        static RAN: AtomicBool = AtomicBool::new(false);
        let handle = spawn(|_ctx| RAN.store(true, Ordering::SeqCst));
        handle.join().expect("spawned thread must not panic");
        assert!(RAN.load(Ordering::SeqCst), "closure should have run");
    }

    #[test]
    fn spawn_returns_joinable_handle() {
        let handle = spawn(|_ctx| {});
        handle.join().expect("empty closure must not panic");
    }

    #[test]
    fn abort_stops_task() {
        static CANCELLED: AtomicBool = AtomicBool::new(false);
        let handle = spawn(|ctx| {
            while !ctx.is_cancelled() {
                std::thread::sleep(Duration::from_millis(1));
            }
            CANCELLED.store(true, Ordering::SeqCst);
        });
        handle.abort();
        handle.join().expect("aborted thread must finish");
        assert!(
            CANCELLED.load(Ordering::SeqCst),
            "task should observe cancellation"
        );
    }

    #[test]
    fn post_update_without_install_errors() {
        struct S;
        impl crate::Component for S {
            fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
                self
            }
            fn render(&self) -> crate::VElement {
                crate::VElement::leaf(
                    "",
                    crate::NodeKind::Text {
                        content: "noop".into(),
                    },
                )
            }
        }
        let result = post_update::<S>(|_| {});
        match result {
            Err(TaskError::NotRunning) => {}
            other => panic!("expected NotRunning, got {other:?}"),
        }
    }

    /// Тест 4.2: Debug-представление `Action` содержит `"Action"`
    /// и `"target"` (TypeId не паникует при форматировании).
    #[test]
    fn action_debug_contains_action_and_target() {
        let action = UserEvent::Action {
            f: Box::new(|_any: &mut dyn Any| {}),
            target: std::any::TypeId::of::<u8>(),
            token: None,
        };
        let s = format!("{action:?}");
        assert!(s.contains("Action"), "Debug must name the variant: {s}");
        assert!(
            s.contains("target"),
            "Debug must include the `target` field: {s}"
        );
        assert!(
            s.contains("<closure>"),
            "Debug must render the closure placeholder: {s}"
        );
    }

    #[test]
    fn task_error_enum_unchanged() {
        // TaskError должен сохранять исходный набор вариантов:
        // NotRunning, Closed, AlreadyInstalled.
        let not_running = TaskError::NotRunning;
        let closed = TaskError::Closed;
        let already = TaskError::AlreadyInstalled;

        assert_eq!(not_running.to_string(), "the event loop is not running");
        assert_eq!(closed.to_string(), "the event loop is closed");
        assert_eq!(
            already.to_string(),
            "the event loop proxy is already installed"
        );
    }
}
