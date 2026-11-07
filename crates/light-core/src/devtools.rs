//! Инспекция состояния приложения: JSON-снимок дерева, реактивных свойств
//! и (в перспективе) раскладки для инструментов разработки (DevTools).
//!
//! При включённой фиче `devtools`:
//! * [`capture_snapshot`] строит снимок текущего состояния компонента;
//! * [`update_latest_snapshot`] / [`read_latest_snapshot`] передают последний
//!   снимок (как JSON-строку) из UI-потока в TCP-сервер;
//! * [`send_command`] / [`drain_commands`] — канал управления: TCP-сервер
//!   отправляет команды (`Click`, `SetState`) в UI-поток, где они
//!   применяются к компоненту;
//! * модуль [`server`] — TCP-сервер на `127.0.0.1`, который отдаёт
//!   последний снимок по команде `SNAPSHOT\n` и принимает управляющие
//!   команды (`CLICK`, `SET_STATE`, `PING`).

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, OnceLock};
#[cfg(all(feature = "async-runtime", feature = "window"))]
use winit::event_loop::EventLoopProxy;

use crate::component::Component;

/// Прокси цикла событий для доставки команд DevTools в UI-поток.
///
/// Устанавливается один раз в [`crate::window::run`] (при включённых
/// `async-runtime` и `window`). Пока он не установлен — команды уходят в
/// глобальную очередь [`COMMAND_QUEUE`], которую UI дренирует при перерисовке
/// (режим «без прокси» нужен для тестов TCP-сервера без запущенного цикла).
#[cfg(all(feature = "async-runtime", feature = "window"))]
static DEVTOOLS_PROXY: OnceLock<EventLoopProxy<crate::async_runtime::UserEvent>> = OnceLock::new();

/// Команда управления, отправляемая из TCP-сервера в UI-поток.
#[derive(Debug)]
pub enum DevToolsCommand {
    /// Симулировать клик по элементу с ключом `key`
    /// (вызывает [`Component::dispatch`] с [`Event::Click`](crate::vdom::Event::Click)).
    ///
    /// Результат обработки (`true` — обработано, `false` — нет) возвращается
    /// в TCP-сервер по каналу `response_tx`; сервер на его основе отвечает
    /// клиенту `ok:true` / `ok:false`.
    Click {
        /// Ключ узла (атрибут `key="..."` в шаблоне).
        key: String,
        /// Канал возврата результата обработки клика.
        response_tx: std::sync::mpsc::Sender<bool>,
    },
    /// Изменить значение реактивного поля `field` на `value`.
    ///
    /// Применяется к компоненту активного окна через
    /// [`Component::inject_state`](crate::component::Component::inject_state);
    /// результат обработки (`true` — окно найдено и перерисовано,
    /// `false` — активного окна нет) возвращается TCP-серверу по каналу
    /// `response_tx`; сервер на его основе отвечает клиенту
    /// `ok:true` / `ok:false`.
    SetState {
        /// Имя поля.
        field: String,
        /// Новое значение (JSON).
        value: serde_json::Value,
        /// Канал возврата результата применения состояния.
        response_tx: std::sync::mpsc::Sender<bool>,
    },
    /// Сохранить текущее состояние компонента активного окна в файл
    /// `.light/state.json` (относительно рабочей директории приложения).
    ///
    /// Команда выполняется асинхронно: UI-поток сам пишет файл при обработке
    /// команды; TCP-сервер отвечает клиенту `ok\n` сразу после отправки
    /// (без ожидания завершения записи). Файл читается при следующем запуске
    /// (`#[cfg(feature = "devtools")]`) для восстановления состояния.
    SaveState,
    /// Перечитать HTML-шаблон с диска и перерисовать все окна без пересборки.
    ///
    /// Используется в режиме `light dev` (runtime-templates): наблюдатель
    /// отправляет `RELOAD_TEMPLATE\n` по TCP, приложение сбрасывает кэш
    /// шаблонов, перечитывает HTML и перерисовывает UI. Команда выполняется
    /// асинхронно; TCP-сервер отвечает клиенту `ok\n` сразу после отправки.
    ReloadTemplate,
    /// ТЗ 0.15.1-fix3: «бережливое» сохранение перед swap-kill. Supervisor
    /// отправляет `SAVE_ALL\n` по TCP **до** TerminateProcess; UI-поток
    /// синхронно сохраняет `state.json` + принудительно пишет `window.json`
    /// (без дебаунса) и возвращает `ok` по каналу `response_tx`. После
    /// ответа supervisor убивает процесс — файл гарантированно актуален.
    SaveAll {
        /// Канал подтверждения: UI-поток отправляет `true` (файлы записаны)
        /// или `false` (ошибка), TCP-сервер ждёт до 1 с и отвечает клиенту.
        response_tx: std::sync::mpsc::Sender<bool>,
    },
}

/// Глобальное хранение последнего JSON-снимка: обновляется из UI-потока при
/// каждой перерисовке, читается TCP-сервером на отдельном потоке.
static LATEST_SNAPSHOT: OnceLock<Arc<Mutex<Option<String>>>> = OnceLock::new();

/// Возвращает (и при необходимости создаёт) глобальное хранилище снимка.
fn latest_snapshot_slot() -> &'static Arc<Mutex<Option<String>>> {
    LATEST_SNAPSHOT.get_or_init(|| Arc::new(Mutex::new(None)))
}

/// Сохраняет последнюю JSON-строку снимка.
///
/// Вызывается из UI-потока (обработчик перерисовки) после каждого кадра.
/// Если `LATEST_SNAPSHOT` ещё не было создано, создаётся автоматически.
pub fn update_latest_snapshot(json: String) {
    let slot = latest_snapshot_slot();
    if let Ok(mut guard) = slot.lock() {
        *guard = Some(json);
    }
}

/// Читает последний JSON-снимок для TCP-сервера.
///
/// Возвращает клон строки или `None`, если снимок ещё не был захвачен.
/// Безопасно вызывать из любого потока.
#[doc(hidden)]
pub(crate) fn read_latest_snapshot() -> Option<String> {
    let slot = latest_snapshot_slot();
    slot.lock().ok()?.clone()
}

// ─── Канал управления: TCP → UI ──────────────────────────────────────────────

/// Глобальная очередь команд управления DevTools. TCP-сервер пушит команды,
/// UI-поток дренирует их при перерисовке. `Mutex` защищает от гонок;
/// критическая секция короткая (push/try_pop), contention не проблема для
/// dev-инструмента.
static COMMAND_QUEUE: OnceLock<Arc<Mutex<VecDeque<DevToolsCommand>>>> = OnceLock::new();

/// Возвращает (и при необходимости создаёт) глобальную очередь команд.
fn command_queue() -> &'static Arc<Mutex<VecDeque<DevToolsCommand>>> {
    COMMAND_QUEUE.get_or_init(|| Arc::new(Mutex::new(VecDeque::new())))
}

/// Отправляет команду управления в очередь для UI-потока.
///
/// Вызывается из TCP-сервера. Если прокси цикла событий уже установлен
/// (`async-runtime` + `window`), команда доставляется напрямую через
/// [`EventLoopProxy`] — это будит спящий цикл, и клик обрабатывается сразу,
/// не дожидаясь перерисовки. В противном случае (прокси нет, например в
/// тестах) команда кладётся в глобальную очередь [`COMMAND_QUEUE`].
#[doc(hidden)]
pub(crate) fn send_command(cmd: DevToolsCommand) {
    #[cfg(all(feature = "async-runtime", feature = "window"))]
    if let Some(proxy) = DEVTOOLS_PROXY.get() {
        let _ = proxy.send_event(crate::async_runtime::UserEvent::DevTools(cmd));
        return;
    }
    let queue = command_queue();
    if let Ok(mut guard) = queue.lock() {
        guard.push_back(cmd);
    }
}

/// Устанавливает прокси цикла событий для доставки команд DevTools.
///
/// Вызывается один раз в [`crate::window::run`]; повторные установки
/// игнорируются (вторая установка просто не выполняется). Нужен, чтобы клики
/// от инспектора обрабатывались сразу, а не при следующей перерисовке окна.
#[doc(hidden)]
#[cfg(all(feature = "async-runtime", feature = "window"))]
pub(crate) fn install_proxy(proxy: &EventLoopProxy<crate::async_runtime::UserEvent>) {
    let _ = DEVTOOLS_PROXY.set(proxy.clone());
}

/// Извлекает все накопленные команды из очереди.
///
/// Вызывается из UI-потока при перерисовке. Команды обрабатываются
/// синхронно, до отрисовки следующего кадра.
///
/// `dead_code` допускается: с прокси цикла событий (`async-runtime` +
/// `window`) команды доставляются сразу и не накапливаются в очереди, но
/// без прокси (например, в тестах TCP-сервера) `send_command` кладёт их сюда —
/// это единственное место, где очередь вообще читается.
#[doc(hidden)]
#[allow(dead_code)]
pub(crate) fn drain_commands() -> Vec<DevToolsCommand> {
    let queue = command_queue();
    let mut cmds = Vec::new();
    if let Ok(mut guard) = queue.lock() {
        while let Some(cmd) = guard.pop_front() {
            cmds.push(cmd);
        }
    }
    cmds
}

/// Снимок состояния интерфейса в формате JSON.
///
/// Передаётся инструментам разработки по WebSocket (задача 0.13-B) или
/// читается из памяти: содержит виртуальное дерево отрисовки, значения
/// реактивных свойств компонента и (опционально) раскладку.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DevToolsSnapshot {
    /// Токен окна, которому принадлежит снимок.
    pub window_token: u32,
    /// Виртуальное дерево ([`crate::vdom::VElement`]) в JSON-представлении.
    pub vdom: serde_json::Value,
    /// Значения реактивных свойств компонента (результат [`Component::inspect_state`]).
    pub state: serde_json::Value,
    /// Данные раскладки, если они доступны; в текущей версии — `None`.
    pub layout: Option<serde_json::Value>,
}

/// Компонент, состояние которого можно инспектировать в DevTools.
///
/// Реализуется автоматически для любого [`Component`] (через blanket impl);
/// конкретные значения реактивных свойств предоставляет реализация
/// [`Component::inspect_state`], генерируемая макросом `#[template]` (фича
/// `devtools`): каждое поле появляется под своим именем со значением геттера.
/// Поля с неподдерживаемым типом отображаются как `null`.
pub trait DevToolsInspect: Component {
    /// Снимок реактивных свойств компонента в формате JSON.
    fn inspect_state(&self) -> serde_json::Value;
}

// Blanket impl: любое `Component` уже имеет `inspect_state` (дефолтный
// возвращает `null`; `#[template]` его переопределяет при включённой фиче).
impl<T: Component + ?Sized> DevToolsInspect for T {
    fn inspect_state(&self) -> serde_json::Value {
        <Self as Component>::inspect_state(self)
    }
}

/// Сериализует одно реактивное свойство для снимка состояния.
///
/// Присутствующие поля сериализуются в соответствующее JSON-значение,
/// отсутствующие (`Option::None`) — как `null`. Используется сгенерированным
/// макросом `#[template]` для каждого поля через геттер; требует
/// `serde::Serialize` у типа значения. Поля, чей тип не сериализуем, не
/// попадают сюда — для них макрос сразу пишет `null` (см. `template.rs`).
pub fn inspect_field<T: serde::Serialize + ?Sized>(value: &T) -> serde_json::Value {
    serde_json::to_value(value).unwrap_or(serde_json::Value::Null)
}

/// Снимает текущее состояние компонента: VDOM и реактивные свойства.
///
/// `token` — идентификатор окна, которому принадлежит снимок
/// (используется в DevTools для адресации). Раскладка пока не включается
/// (`layout = None`).
pub fn capture_snapshot(token: u32, component: &dyn Component) -> DevToolsSnapshot {
    DevToolsSnapshot {
        window_token: token,
        vdom: component.render().to_json(),
        state: component.inspect_state(),
        layout: None,
    }
}

/// Путь файла состояния относительно рабочей директории приложения.
///
/// Приложение (запущенное `light dev` или `cargo run`) работает из корня
/// проекта, поэтому относительный путь `.light/state.json` указывает на каталог
/// проекта; при необходимости можно переопределить переменной окружения
/// `LIGHT_STATE_FILE` (например, в тестах — на временный файл).
pub const STATE_FILE: &str = ".light/state.json";

/// Возвращает фактический путь файла состояния.
///
/// Если переменная окружения `LIGHT_STATE_FILE` задана и непуста — её значение,
/// иначе [`STATE_FILE`] (относительно текущей рабочей директории).
pub fn state_file_path() -> std::path::PathBuf {
    std::env::var_os("LIGHT_STATE_FILE")
        .map(std::path::PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::PathBuf::from(STATE_FILE))
}

/// Сохраняет JSON-объект состояния компонента в файл [`state_file_path`].
///
/// Создаёт родительский каталог (`.light/`), если его ещё нет. Ошибки записи
/// не являются критическими для UI: они логируются в stderr, а функция
/// возвращает `Result`, чтобы вызывающая сторона могла отреагировать
/// (например, сообщить инспектору через `SET_STATE`/`SAVE_STATE`).
///
/// Вызывается из UI-потока: при закрытии окна и по команде `SAVE_STATE`
/// из DevTools.
pub fn save_state_file(state: &serde_json::Value) -> std::io::Result<()> {
    let path = state_file_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(state).map_err(std::io::Error::other)?;
    std::fs::write(&path, json)?;
    Ok(())
}

/// Читает ранее сохранённое состояние компонента из [`state_file_path`].
///
/// Возвращает `Ok(Some(value))`, если файл существует и успешно разобран в
/// JSON-объект; `Ok(None)` — если файла нет (нормальный первый запуск);
/// `Err(..)` — при ошибке чтения или разбора (файл повреждён). Повреждённый
/// файл не блокирует запуск: вызывающая сторона логирует ошибку и продолжает
/// с чистым состоянием.
pub fn load_state_file() -> Result<Option<serde_json::Value>, std::io::Error> {
    let path = state_file_path();
    if !path.exists() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(&path)?;
    let value: serde_json::Value = serde_json::from_str(&raw).map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("не удалось разобрать {}: {e}", path.display()),
        )
    })?;
    Ok(Some(value))
}

/// TCP-сервер для DevTools: отдаёт последний JSON-снимок по построчному
/// протоколу (`SNAPSHOT\n` → JSON, `QUIT\n` → закрыть соединение).
pub mod server;
