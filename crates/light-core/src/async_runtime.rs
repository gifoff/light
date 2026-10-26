//! Опциональная асинхронная обвязка: фоновые задачи и отправка в цикл событий.
//!
//! Фича `async-runtime` добавляет к библиотеке два простых инструмента:
//!
//! * [`spawn`] — запускает замыкание в отдельном потоке `std::thread`
//!   (без внешних рантаймов: ни `tokio`, ни `async/await` не нужны);
//! * [`post_to_ui`] — ставит в очередь событие цикла, которое выполнит
//!   замыкание в потоке пользовательского интерфейса и запросит перерисовку
//!   открытых окон.
//!
//! Отправка работает из любого фонового потока (не только из реактивного
//! наблюдателя): глобальный прокси цикла событий регистрируется при
//! запуске [`crate::window::run`] и переживает переключение контекстов
//! `thread_local`.

use std::sync::{Arc, Mutex, OnceLock};
use std::thread::{Builder, JoinHandle};

use winit::event_loop::EventLoopProxy;

use crate::window::UserEvent;

type UiProxy = Arc<Mutex<Option<EventLoopProxy<UserEvent>>>>;

/// Глобальный прокси цикла событий; заполняется при запуске окна.
static PROXY: OnceLock<UiProxy> = OnceLock::new();

/// Ошибка асинхронной обвязки.
#[derive(Debug, thiserror::Error)]
pub enum TaskError {
    /// Задача не была отправлена: цикл событий ещё не запущен
    /// (вызов до `run()`) или окно уже закрыто.
    #[error("цикл событий не запущен или окно уже закрыто")]
    NotRunning,
}

/// Дескриптор фоновой задачи.
///
/// Закрыт над [`JoinHandle`] и предоставляет доступ к имени потока
/// и ожидания его завершения.
pub struct TaskHandle(JoinHandle<()>);

impl TaskHandle {
    /// Имя потока задачи (если оно было задано).
    pub fn thread_name(&self) -> Option<&str> {
        self.0.thread().name()
    }

    /// Ждёт завершения задачи; результат — `Ok(())`, т.к. `spawn` не возвращает значений.
    pub fn join(self) -> std::io::Result<()> {
        self.0.join().map_err(|payload| {
            // `JoinHandle::join()` отдаёт панику как `Box<dyn Any + Send>`,
            // который нельзя напрямую передать в `io::Error::other`
            // (там требуется `Error + Send + Sync`). Форматируем сообщение
            // по типу паникующей ценности (обычно это `String` или `&str`).
            let msg = if let Some(s) = payload.downcast_ref::<String>() {
                s.clone()
            } else if let Some(s) = payload.downcast_ref::<&str>() {
                s.to_string()
            } else {
                format!(
                    "panicked at {:?}(type: {})",
                    payload,
                    std::any::type_name::<dyn std::any::Any + Send>()
                )
            };
            std::io::Error::other(msg)
        })
    }
}

/// Устанавливает глобальный прокси цикла событий (вызывается из `window::run`).
///
/// Повторная установка игнорируется, как в [`crate::control::set_command_sender`].
pub(crate) fn set_event_loop_proxy(proxy: EventLoopProxy<UserEvent>) {
    let _ = PROXY.set(Arc::new(Mutex::new(Some(proxy))));
}

/// Снимает глобальный прокси (вызывается при закрытии окна).
pub(crate) fn clear_event_loop_proxy() {
    if let Some(p) = PROXY.get() {
        *p.lock().expect("proxy poisoned") = None;
    }
}

/// Запускает замыкание в отдельном потоке.
///
/// Возвращает [`TaskHandle`] для ожидания завершения.
///
/// # Пример
///
/// ```no_run
/// use light_fw_core::async_runtime::spawn;
///
/// let handle = spawn(|| println!("выполняюсь в фоне"));
/// let _ = handle.join();
/// ```
pub fn spawn<F>(f: F) -> TaskHandle
where
    F: FnOnce() + Send + 'static,
{
    let handle = Builder::new()
        .name("light-async".to_string())
        .spawn(f)
        .expect("не удалось создать поток");
    TaskHandle(handle)
}

/// Ставит замыкание в очередь в поток пользовательского интерфейса.
///
/// Замыкание будет выполнено в потоке цикла событий при следующем
/// обработке события [`UserEvent::User`], после чего будет запрошена
/// перерисовка всех открытых окон.
///
/// Возвращает [`Err(TaskError::NotRunning)`], если цикл событий ещё
/// не запущен или окно уже закрыто.
///
/// # Пример
///
/// ```no_run
/// use light_fw_core::async_runtime::{spawn, post_to_ui};
///
/// let handle = spawn(|| {
///     let _ = post_to_ui(|| {
///         // здесь можно трогать UI-состояние:
///         // свойство будет обновлено в потоке UI
///     });
/// });
/// ```
pub fn post_to_ui(f: impl FnOnce() + Send + 'static) -> Result<(), TaskError> {
    let Some(proxy) = PROXY.get() else {
        return Err(TaskError::NotRunning);
    };
    let mut guard = proxy.lock().expect("proxy poisoned");
    let Some(proxy) = guard.as_mut() else {
        return Err(TaskError::NotRunning);
    };
    proxy
        .send_event(UserEvent::User(Box::new(f)))
        .map_err(|_| TaskError::NotRunning)
}
