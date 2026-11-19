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

use std::sync::OnceLock;
#[cfg(all(feature = "async-runtime", feature = "window"))]
use winit::event_loop::EventLoopProxy;

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
    /// (вызывает [`Component::dispatch`] с [`Event::Click`](light_vdom::Event::Click)).
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
    /// [`Component::inject_state`](light_vdom::component::Component::inject_state);
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
    /// ТЗ 0.16.0: ввести текст в текстовое поле с ключом `key`.
    ///
    /// Фокусирует поле, заменяет его содержимое на `text` и применяет
    /// изменение через `commit_text`. Результат (`"ok"` или
    /// `"error: <msg>"`) возвращается TCP-серверу по каналу `response_tx`.
    Type {
        /// Ключ узла текстового поля.
        key: String,
        /// Текст, который нужно ввести (заменяет текущее содержимое).
        text: String,
        /// Канал возврата: `Some("ok")` / `Some("error: ...")`.
        response_tx: std::sync::mpsc::Sender<Option<String>>,
    },
    /// ТЗ 0.16.0: отправить системный ключ (Enter / Tab / Escape) в активное
    /// окно, адресованный элементу с ключом `key`.
    ///
    /// Имитирует нажатие+отпускание ключа без построения `winit::KeyEvent`
    /// (в 0.30 у него приватные поля): диспатчит `Keydown`/`KeyUp` в
    /// компонент и обрабатывает семантику Enter (submit), Escape (закрыть
    /// select). Результат возвращается по `response_tx`.
    Key {
        /// Ключ узла, которому адресован ключ.
        key: String,
        /// Имя ключа: "Enter" | "Tab" | "Escape".
        keyname: String,
        /// Канал возврата: `Some("ok")` / `Some("error: ...")`.
        response_tx: std::sync::mpsc::Sender<Option<String>>,
    },
    /// ТЗ 0.16.0: вернуть VDOM-дерево активного окна в ТЗ-формате
    /// (`{"key","tag","text","children"}`).
    ///
    /// Обрабатывается в UI-потоке (нужен живой [`light_vdom::VElement`],
    /// JSON-снимок не содержит текста). Результат — JSON-строка или `null`.
    Tree {
        /// Канал возврата: `Some(json)` / `Some("null")`.
        response_tx: std::sync::mpsc::Sender<Option<String>>,
    },
    /// ТЗ 0.16.0: вернуть дерево раскладки активного окна
    /// (`{"key","tag","x","y","w","h","children"}`).
    LayoutCmd {
        /// Канал возврата: `Some(json)` / `Some("null")`.
        response_tx: std::sync::mpsc::Sender<Option<String>>,
    },
    /// ТЗ 0.16.0: найти элемент по JSON-запросу и вернуть его key/tag/rect.
    Find {
        /// JSON-запрос: `{"tag":..,"text":..,"attr":{k:v}}`.
        query: String,
        /// Канал возврата: `Some(json)` / `Some("null")`.
        response_tx: std::sync::mpsc::Sender<Option<String>>,
    },
    /// ТЗ 0.16.0: захватить кадр активного окна (PNG → base64).
    Shot {
        /// Канал возврата: `Some("base64:...")` / `Some("error: ...")`.
        response_tx: std::sync::mpsc::Sender<Option<String>>,
    },
}

/// Сериализация дерева и раскладки для DevTools.
pub mod serialize;
/// TCP-сервер для DevTools: отдаёт последний JSON-снимок по построчному
/// протоколу (`SNAPSHOT\n` → JSON, `QUIT\n` → закрыть соединение).
pub mod server;
/// Снимок состояния: дерево, состояние и (опционально) раскладка.
pub mod snapshot;

pub use serialize::*;
pub use snapshot::*;

#[cfg(test)]
mod tests;
