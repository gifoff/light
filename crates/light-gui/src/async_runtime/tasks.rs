use std::any::{Any, TypeId};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

#[cfg(feature = "async-await")]
use std::future::Future;

use light_vdom::component::Component;
use light_vdom::window_backend::WindowToken;

use super::types::{TaskContext, TaskHandle, UserEvent, TaskError, UpdateFn, UI_PROXY};

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

/// Запускает асинхронный замыкание в отдельном потоке ОС, блокируя его
/// до завершения через [`futures::executor::block_on`].
///
/// Замыкание принимает [`TaskContext`] и возвращает `Future`, что даёт
/// доступ к кооперативному флагу отмены внутри асинхронного кода — ровно
/// как в [`spawn`], но для `async`-фьюч. Возвращает [`TaskHandle`].
///
/// Важно: [`futures::executor::block_on`] не поддерживает принудительную
/// отмену извне — если фьюча висит на I/O, [`TaskHandle::abort`] её не
/// прервёт. Отмена проверяется вручную через `ctx.is_cancelled()`.
/// Это ожидаемое поведение для 0.12.0.
///
/// # Пример
///
/// ```rust
/// # #[cfg(feature = "async-await")]
/// # fn example() {
/// use light_gui::async_runtime::spawn_async;
///
/// spawn_async(|ctx| async move {
///     // асинхронная работа с доступом к флагу отмены
///     if ctx.is_cancelled() { return; }
///     # std::future::ready(()).await;
/// });
/// # }
/// ```
#[cfg(feature = "async-await")]
pub fn spawn_async<F, Fut>(f: F) -> TaskHandle
where
    F: FnOnce(TaskContext) -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    let cancel_flag = Arc::new(AtomicBool::new(false));
    let ctx = TaskContext {
        cancel_flag: cancel_flag.clone(),
    };

    let inner = std::thread::Builder::new()
        .name("light-async-await".into())
        .spawn(move || {
            futures::executor::block_on(f(ctx));
        })
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

/// Применяет действие [`UserEvent::Action`] к одному из открытых окон.
///
/// Логика доставки, вынесенная из обработчика событий окна
/// ([`crate::window::App::user_event`]) в отдельную функцию, чтобы её можно
/// было проверить без запуска цикла событий winit. Окна передаются
/// итератором пар `(токен, компонент)` — именно эти два поля использует
/// доставка; остальное состояние окна (поверхность, раскладка) тут не
/// трогается.
///
/// * `token = Some(t)` — точная адресация: действие применяется только к
///   окну с токеном `t`; если такого окна нет (в том числе уже закрыто),
///   возвращается `None`, и действие отбрасывается;
/// * `token = None` — действие применяется к первому окну, чей компонент
///   имеет `TypeId` `target`; если таких окон нет, возвращается `None`.
///
/// Возвращает `Some(токен)` того окна, к которому действие было применено —
/// обработчик использует его для `request_redraw()` целевого окна. Перерисовка
/// не запрашивается внутри функции, что устраняет конфликт заёмов между
/// итератором компонентов и доступом к `Arc<Window>`.
/// Публичный API для тестирования логики доставки [`UserEvent::Action`]
/// без запуска цикла событий winit. См. внутреннюю документацию выше.
#[cfg(feature = "async-runtime")]
pub fn apply_action<'a>(
    windows: impl Iterator<Item = (WindowToken, TypeId, &'a mut dyn Component)>,
    token: Option<WindowToken>,
    target: TypeId,
    f: UpdateFn,
) -> Option<WindowToken> {
    // Для точной адресации (Some(token)) ищем по токену — TypeId не нужен.
    // Для broadcast (None) ищем по TypeId; он передаётся вместе с парой,
    // чтобы не вызывать as_any_mut() дважды (одна mutable borrow не может
    // служить и для type_id(), и для f).
    for (window_token, type_id, component) in windows {
        let matched = match token {
            Some(t) => window_token == t,
            None => type_id == target,
        };
        if matched {
            f(component.as_any_mut());
            return Some(window_token);
        }
    }
    None
}

/// Ставит типизированное обновление компонента в очередь UI-цикла
/// с адресацией к конкретному окну по его [`WindowToken`].
///
/// Замыкание выполнится в UI-потоке и применится **только** к окну,
/// чей токен совпадает с `token`. Если окно с этим токеном уже закрыто,
/// обновление тихо отбрасывается (в UI-потоке логируется
/// `async-runtime: target window closed`).
///
/// Токен окна получается через [`light_vdom::window_backend::current_window_token`]
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

