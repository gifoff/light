//! Канал команд управления окном.
//!
//! Шаблоны превращают `@click="close()"` в вызов `self.close()`. Методы
//! управления окном объявлены в трейте [`crate::component::Component`] с
//! реализацией по умолчанию, которая отправляет команду в этот канал.
//! Оконный бэкенд регистрирует получателя команд ([`set_command_sender`]).

use std::cell::Cell;
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
