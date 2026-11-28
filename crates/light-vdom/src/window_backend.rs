//! Канал команд управления окном и состояние окна.
//!
//! Этот модуль владеет всей «оконной» логикой VDOM-слоя: токен активного
//! окна, кросспоточный токен текущего окна, флаг развёрнутости и очередь
//! команд (`WindowCommand`). Оконный бэкенд (`light-fw-core::window`)
//! регистрирует получателя команд через [`set_command_sender`] и опрашивает
//! очередь через [`take_command`].
//!
//! **Важно:** этот модуль НЕ зависит от `winit` или любого другого оконного
//! бэкенда. Он чистая логика, которую может использовать любой бэкенд.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

/// Непрозрачный идентификатор окна, присваиваемый оконным бэкендом.
///
/// Используется, чтобы команда, отправленная из обработчика события, попала в
/// то окно, где произошло событие. Значение `0` зарезервировано: оно означает
/// «окно не задано», и [`current_window_token`] возвращает `None`, пока
/// токеном не установлено ненулевое значение. Оконный бэкенд начинает
/// нумерацию реальных окон с `1`, поэтому у каждого созданного окна
/// ненулевой токен.
pub type WindowToken = u64;

thread_local! {
    /// Токен окна, событие которого обрабатывается в данный момент.
    static ACTIVE: Cell<WindowToken> = const { Cell::new(0) };
}

/// Запоминает окно, событие которого сейчас обрабатывается.
///
/// Вызывается оконным бэкендом перед обработкой события окна, чтобы методы
/// `VWindow::close` и подобные адресовались нужному окну.
pub fn set_active_window(token: WindowToken) {
    ACTIVE.with(|active| active.set(token));
}

/// Токен окна, событие которого обрабатывается в данный момент.
///
/// Вне обработки события возвращается `0` («окно не задано»); реальные окна
/// получают ненулевые токены (см. [`WindowToken`]).
pub fn active_window() -> WindowToken {
    ACTIVE.with(Cell::get)
}

/// Кросспоточный токен «текущего» окна.
///
/// Значение `0` означает «окно не задано»: [`current_window_token`]
/// возвращает `None`, пока ни одно окно не установило ненулевой токен.
/// Оконный бэкенд начинает нумерацию реальных окон с `1` (см.
/// [`WindowToken`]).
static CURRENT_WINDOW: AtomicU64 = AtomicU64::new(0);

/// Запоминает токен «текущего» окна для кросспоточного доступа.
///
/// Вызывается оконным бэкендом в UI-потоке, перед обработкой события окна.
/// Позволяет задачам, запущенным из обработчика (`post_update_to`), узнать,
/// из какого окна они пришли, даже из другого потока.
pub fn set_current_window_token(token: WindowToken) {
    CURRENT_WINDOW.store(token, Ordering::Relaxed);
}

/// Токен «текущего» окна для кросспоточного доступа.
///
/// В отличие от [`active_window`], этот токен доступен из любого потока
/// (включая задачи, запущенные из обработчика события), и возвращает
/// `None`, пока ни одно окно не установило токен.
pub fn current_window_token() -> Option<WindowToken> {
    let token = CURRENT_WINDOW.load(Ordering::Relaxed);
    (token != 0).then_some(token)
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
    /// Принудительно перерисовать окно (без изменения состояния видимости).
    ///
    /// Используется виджетами (например, [`Toast`]) для гарантии, что
    /// изменения состояния в UI-потоке через `post_to_ui` приведут к
    /// реальной перерисовке кадра даже без активных событий ввода.
    Invalidate,
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

/// Очередь команд для оконного бэкенда (fallback, если `SENDER` не установлен).
static COMMAND_QUEUE: Mutex<VecDeque<(WindowToken, WindowCommand)>> = Mutex::new(VecDeque::new());

/// Добавляет команду в очередь (используется, если `SENDER` не зарегистрирован).
pub fn enqueue(command: WindowCommand) {
    let token = ACTIVE.with(Cell::get);
    let mut queue = COMMAND_QUEUE.lock().unwrap();
    queue.push_back((token, command));
}

/// Извлекает все накопленные команды (вызывает оконный бэкенд каждый кадр).
pub fn take_command() -> Vec<(WindowToken, WindowCommand)> {
    let mut queue = COMMAND_QUEUE.lock().unwrap();
    queue.drain(..).collect()
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
