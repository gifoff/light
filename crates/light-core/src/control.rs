//! Канал команд управления окном.
//!
//! Шаблоны превращают `@click="close()"` в вызов `self.close()`. Методы
//! управления окном объявлены в трейте [`crate::component::Component`] с
//! реализацией по умолчанию, которая отправляет команду в этот канал.
//! Оконный бэкенд регистрирует получателя команд ([`set_command_sender`]).

use std::cell::{Cell, RefCell};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

/// Непрозрачный идентификатор окна, присваиваемый оконным бэкендом.
///
/// Используется, чтобы команда, отправленная из обработчика события, попала в
/// то окно, где произошло событие. Значение `0` зарезервировано под «окно по
/// умолчанию» (первое созданное).
pub type WindowToken = u64;

thread_local! {
    /// Токен окна, событие которого обрабатывается в данный момент.
    static ACTIVE: Cell<WindowToken> = const { Cell::new(0) };
}

/// Запоминает окно, событие которого сейчас обрабатывается.
///
/// Вызывается оконным бэкендом перед обработкой события окна, чтобы методы
/// [`crate::vdom::VWindow::close`] и подобные адресовались нужному окну.
pub fn set_active_window(token: WindowToken) {
    ACTIVE.with(|active| active.set(token));
}

/// Токен окна, событие которого обрабатывается в данный момент.
///
/// Вне обработки события возвращается последнее активное окно (по умолчанию
/// `0` — первое созданное).
pub fn active_window() -> WindowToken {
    ACTIVE.with(Cell::get)
}

/// Развёрнуто ли окно в данный момент (обновляет оконный бэкенд).
static MAXIMIZED: AtomicBool = AtomicBool::new(false);

/// Обновляет состояние развёрнутости окна.
///
/// Вызывается оконным бэкендом при каждом изменении состояния окна, чтобы
/// шаблоны могли реагировать на него (`v-if="is_maximized"`).
pub fn set_maximized(value: bool) {
    MAXIMIZED.store(value, Ordering::Relaxed);
}

/// Развёрнуто ли окно в данный момент.
pub fn is_maximized() -> bool {
    MAXIMIZED.load(Ordering::Relaxed)
}

/// Команда управления окном.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WindowCommand {
    /// Закрыть окно.
    Close,
    /// Свернуть окно.
    Minimize,
    /// Развернуть окно.
    Maximize,
    /// Вернуть окно в обычный размер.
    Restore,
    /// Переключить развёрнутость окна.
    ToggleMaximize,
    /// Показать скрытое окно.
    Show,
    /// Скрыть окно.
    Hide,
    /// Открыть дочернее модальное окно поверх текущего.
    OpenModal,
    /// Открыть ещё одно главное окно приложения.
    OpenWindow,
    /// Открыть (или сфокусировать) окно с указанным идентификатором.
    Open(String),
    /// Закрыть окно с указанным идентификатором (вместе с потомками).
    CloseWindow(String),
    /// Показать окно с указанным идентификатором.
    ShowWindow(String),
    /// Скрыть окно с идентификатором.
    HideWindow(String),
}

type Sender = Box<dyn FnMut(WindowToken, WindowCommand) + Send + 'static>;

static SENDER: OnceLock<Mutex<Sender>> = OnceLock::new();

/// Регистрирует получателя команд окна.
///
/// Вызывается оконным бэкендом при запуске цикла событий; повторная
/// регистрация игнорируется.
pub fn set_command_sender(sender: impl FnMut(WindowToken, WindowCommand) + Send + 'static) {
    let _ = SENDER.set(Mutex::new(Box::new(sender)));
}

/// Отправляет команду окну, событие которого обрабатывается в данный момент.
///
/// Если оконный бэкенд не запущен, команда молча отбрасывается.
pub fn request(command: WindowCommand) {
    if let Some(sender) = SENDER.get()
        && let Ok(mut sender) = sender.lock()
    {
        let token = ACTIVE.with(Cell::get);
        sender(token, command);
    }
}

/// Программный запрос клавиатурного фокуса от компонента.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FocusCommand {
    /// Установить фокус на узел с указанным ключом.
    Focus(String),
    /// Снять фокус с текущего узла.
    Blur,
}

thread_local! {
    /// Ожидающий запрос фокуса (потребляется оконным бэкендом).
    static FOCUS_COMMAND: RefCell<Option<FocusCommand>> = const { RefCell::new(None) };
    /// Ключ узла в фокусе, опубликованный оконным бэкендом.
    static FOCUSED_KEY: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Запрашивает клавиатурный фокус на узле с ключом `key`.
///
/// Оконный бэкенд применяет запрос при ближайшей перерисовке. Если фокус уже
/// на этом узле, запрос безвреден.
pub fn focus(key: impl Into<String>) {
    FOCUS_COMMAND.with(|slot| *slot.borrow_mut() = Some(FocusCommand::Focus(key.into())));
}

/// Запрашивает снятие клавиатурного фокуса.
pub fn blur() {
    FOCUS_COMMAND.with(|slot| *slot.borrow_mut() = Some(FocusCommand::Blur));
}

/// Ключ узла, находящегося в клавиатурном фокусе в данный момент.
///
/// Значение публикует оконный бэкенд; без запущенного окна возвращается
/// последнее известное (по умолчанию `None`).
pub fn focused_key() -> Option<String> {
    FOCUSED_KEY.with(|key| key.borrow().clone())
}

/// Извлекает ожидающий запрос фокуса (вызывает оконный бэкенд).
pub fn take_focus_command() -> Option<FocusCommand> {
    FOCUS_COMMAND.with(|slot| slot.borrow_mut().take())
}

/// Публикует ключ узла в фокусе (вызывает оконный бэкенд).
pub fn set_focused_key(key: Option<String>) {
    FOCUSED_KEY.with(|slot| *slot.borrow_mut() = key);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focus_request_round_trips() {
        set_focused_key(None);
        assert_eq!(focused_key(), None);
        assert_eq!(take_focus_command(), None);

        focus("field");
        assert_eq!(
            take_focus_command(),
            Some(FocusCommand::Focus("field".into()))
        );
        assert_eq!(take_focus_command(), None, "запрос потребляется один раз");

        blur();
        assert_eq!(take_focus_command(), Some(FocusCommand::Blur));

        set_focused_key(Some("field".into()));
        assert_eq!(focused_key().as_deref(), Some("field"));
        set_focused_key(None);
    }
}
