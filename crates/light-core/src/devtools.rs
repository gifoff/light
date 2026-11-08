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
    /// Обрабатывается в UI-потоке (нужен живой [`crate::vdom::VElement`],
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

// ─── ТЗ 0.16.0: агентовые инструменты (TREE / LAYOUT / FIND / SHOT) ─────────

/// ТЗ 0.16.0: сериализует RGBA8-буфер в PNG и кодирует его в base64.
///
/// Возвращает строку `base64:<data>` (без `\n` — сервер добавит сам).
/// Используется командой `SHOT\n`.
#[cfg(feature = "devtools")]
pub fn encode_screenshot(buf: &[u8], width: u32, height: u32) -> String {
    use base64::Engine;
    let mut png_data = Vec::new();
    let mut encoder = png::Encoder::new(&mut png_data, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .expect("png header")
        .write_image_data(buf)
        .expect("png write");
    let b64 = base64::engine::general_purpose::STANDARD.encode(&png_data);
    format!("base64:{b64}")
}

/// ТЗ 0.16.0: сериализует VDOM-дерево в компактный JSON для команды `TREE`.
///
/// Формат (по ТЗ): `{"key":"...","tag":"..."|null,"text":null|"...","children":[...]}`.
/// Текстовое содержимое берётся только из `NodeKind::Text`; у контейнеров
/// `text = null`, а `tag` — атрибут `tag` элемента (если задан), иначе `null`.
pub fn serialize_tree(element: &crate::vdom::VElement) -> serde_json::Value {
    serialize_tree_inner(element)
}

fn serialize_tree_inner(el: &crate::vdom::VElement) -> serde_json::Value {
    use crate::vdom::NodeKind;
    let tag = if el.tag.is_empty() {
        None
    } else {
        Some(serde_json::Value::String(el.tag.to_owned()))
    };
    let text = match &el.kind {
        NodeKind::Text { content } => Some(serde_json::Value::String(content.clone())),
        _ => None,
    };
    let children: Vec<serde_json::Value> = el.children.iter().map(serialize_tree_inner).collect();
    let mut obj = serde_json::Map::new();
    obj.insert("key".into(), serde_json::Value::String(el.key.to_owned()));
    obj.insert("tag".into(), tag.unwrap_or(serde_json::Value::Null));
    obj.insert("text".into(), text.unwrap_or(serde_json::Value::Null));
    obj.insert("children".into(), serde_json::Value::Array(children));
    serde_json::Value::Object(obj)
}

/// ТЗ 0.16.0: сериализует плоское дерево раскладки в JSON для команды
/// `LAYOUT`.
///
/// Раскладка хранится как плоский список [`crate::layout::PlacedNode`] с
/// `children: Vec<usize>` (индексы). Рекурсируем от узла с индексом 0
/// (корень), формируя `{"key":..,"tag":..,"x":..,"y":..,"w":..,"h":..,
/// "children":[...]}`. `tag` берётся из VDOM-дерева по ключу (если найден).
pub fn serialize_layout(
    layout: &crate::layout::Layout,
    tree: Option<&crate::vdom::VElement>,
) -> serde_json::Value {
    let nodes = layout.nodes();
    if nodes.is_empty() {
        return serde_json::Value::Null;
    }
    // Корень — узел с key "0" (первый элемент дерева) или индекс 0.
    // В нашей раскладке корень всегда индекс 0.
    serialize_layout_node(nodes, tree, 0, 0)
}

fn serialize_layout_node(
    nodes: &[crate::layout::PlacedNode],
    tree: Option<&crate::vdom::VElement>,
    index: usize,
    depth: usize,
) -> serde_json::Value {
    if depth > 100 || index >= nodes.len() {
        return serde_json::Value::Null;
    }
    let node = &nodes[index];
    // Tag: ищем элемент в VDOM по ключу.
    let tag = tree.and_then(|t| t.get(node.key)).and_then(|el| {
        if el.tag.is_empty() {
            None
        } else {
            Some(el.tag.to_owned())
        }
    });
    let rect = node.rect;
    let children: Vec<serde_json::Value> = node
        .children
        .iter()
        .map(|&ci| serialize_layout_node(nodes, tree, ci, depth + 1))
        .collect();
    let mut obj = serde_json::Map::new();
    obj.insert("key".into(), serde_json::Value::String(node.key.to_owned()));
    obj.insert(
        "tag".into(),
        tag.map(serde_json::Value::String)
            .unwrap_or(serde_json::Value::Null),
    );
    obj.insert("x".into(), serde_json::json!(rect.x));
    obj.insert("y".into(), serde_json::json!(rect.y));
    obj.insert("w".into(), serde_json::json!(rect.w));
    obj.insert("h".into(), serde_json::json!(rect.h));
    obj.insert("children".into(), serde_json::Value::Array(children));
    serde_json::Value::Object(obj)
}

/// ТЗ 0.16.0: выполняет поиск элемента по запросу (таг, текст, атрибуты).
///
/// Запрос — JSON-объект вида `{"tag":"button","text":"+1","attr":{"class":"btn"}}`.
/// Поля необязательны; совпадение — точное. Возвращает
/// `{"key":..,"tag":..,"rect":{"x":..,"y":..,"w":..,"h":..}}` или `None`,
/// если элемент не найден.
pub fn find_element(
    tree: &crate::vdom::VElement,
    layout: Option<&crate::layout::Layout>,
    query: &str,
) -> Option<serde_json::Value> {
    let q: serde_json::Value = serde_json::from_str(query).ok()?;
    let want_tag = q.get("tag").and_then(|v| v.as_str());
    let want_text = q.get("text").and_then(|v| v.as_str());
    let want_attrs: Vec<(String, String)> = q
        .get("attr")
        .and_then(|a| a.as_object())
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_owned())))
                .collect()
        })
        .unwrap_or_default();

    for el in tree.descendants() {
        if let Some(want_tag) = want_tag
            && el.tag != want_tag
        {
            continue;
        }
        if let Some(want_text) = want_text {
            // Текст узла: у текстовых узлов — собственный `content`; у
            // контейнеров (кнопки, поля) — конкатенация текстовых потомков
            // ВКЛЮЧИТЕЛЬНО через кнопку/поле (их подпись — это текст внутри).
            // Это позволяет искать кнопку по её подписи:
            // `{"tag":"button","text":"+1"}`.
            let mut buf = String::new();
            collect_match_text(el, &mut buf);
            if buf != want_text {
                continue;
            }
        }
        if !want_attrs.is_empty() {
            let all_match = want_attrs
                .iter()
                .all(|(k, v)| el.attr(k) == Some(v.as_str()));
            if !all_match {
                continue;
            }
        }
        // Найдено — собираем ответ с rect из раскладки.
        let tag = if el.tag.is_empty() {
            None
        } else {
            Some(el.tag.to_owned())
        };
        let rect = layout
            .and_then(|l| l.node(el.key))
            .map(|n| {
                serde_json::json!({
                    "x": n.rect.x,
                    "y": n.rect.y,
                    "w": n.rect.w,
                    "h": n.rect.h,
                })
            })
            .unwrap_or(serde_json::json!({"x": 0, "y": 0, "w": 0, "h": 0}));
        let mut obj = serde_json::Map::new();
        obj.insert("key".into(), serde_json::Value::String(el.key.to_owned()));
        obj.insert(
            "tag".into(),
            tag.map(serde_json::Value::String)
                .unwrap_or(serde_json::Value::Null),
        );
        obj.insert("rect".into(), rect);
        return Some(serde_json::Value::Object(obj));
    }
    None
}

/// Собирает текст поддерева для поиска `FIND`: текстовые узлы дают свой
/// `content`, всё остальное — суммарный текст потомков (включая кнопки/поля,
/// в отличие от [`VElement::collect_plain_text_public`], которая их
/// пропускает). Это нужно, чтобы найти кнопку по её подписи.
fn collect_match_text(el: &crate::vdom::VElement, out: &mut String) {
    use crate::vdom::NodeKind;
    if let NodeKind::Text { content } = &el.kind {
        out.push_str(content);
        return;
    }
    for child in &el.children {
        collect_match_text(child, out);
    }
}

/// TCP-сервер для DevTools: отдаёт последний JSON-снимок по построчному
/// протоколу (`SNAPSHOT\n` → JSON, `QUIT\n` → закрыть соединение).
pub mod server;

#[cfg(test)]
mod tests {
    //! Юнит-тесты ТЗ 0.16.0: сериализаторы `TREE`/`LAYOUT`, поиск `FIND`,
    //! и кодирование `SHOT`.

    use super::*;
    use crate::layout::{Layout, TextMetrics};
    use crate::vdom::{ButtonType, NodeKind, VElement};

    /// Заглушка метрик текста: фиксированная ширина/высота (для тестов
    /// раскладки не важно — важны ключи и иерархия узлов).
    struct StubMetrics;
    impl TextMetrics for StubMetrics {
        fn measure(&self, _text: &str, font_size: f32) -> (f32, f32) {
            (font_size * 4.0, font_size * 1.5)
        }
    }

    /// Дерево для тестов: `window > div > [button "+1", text "Count"]`.
    fn sample_tree() -> VElement {
        VElement::container(
            "0",
            NodeKind::Window,
            vec![
                VElement::container(
                    "0.0",
                    NodeKind::Column,
                    vec![
                        VElement::container(
                            "0.0.0",
                            NodeKind::Button {
                                disabled: false,
                                button_type: ButtonType::default(),
                            },
                            vec![
                                VElement::leaf(
                                    "0.0.0.0",
                                    NodeKind::Text {
                                        content: "+1".to_owned(),
                                    },
                                )
                                .with_tag("span"),
                            ],
                        )
                        .with_tag("button"),
                        VElement::leaf(
                            "0.1",
                            NodeKind::Text {
                                content: "Count".to_owned(),
                            },
                        ),
                    ],
                )
                .with_tag("div"),
            ],
        )
        .with_tag("window")
        .with_window(crate::component::WindowConfig::default())
    }

    /// `TREE`: текстовые узлы несут `text`, контейнеры — `tag` и пустой `text`.
    #[test]
    fn serialize_tree_shape() {
        let v = serialize_tree(&sample_tree());
        assert_eq!(v["key"], "0");
        assert_eq!(v["tag"], serde_json::json!("window"));
        assert_eq!(v["text"], serde_json::Value::Null);
        let div = &v["children"][0];
        assert_eq!(div["key"], "0.0");
        assert_eq!(div["tag"], serde_json::json!("div"));
        let button = &div["children"][0];
        assert_eq!(button["tag"], serde_json::json!("button"));
        // Кнопка — контейнер: собственный text = null (подпись — в потомках).
        assert_eq!(button["text"], serde_json::Value::Null);
        let text = &div["children"][1];
        assert_eq!(text["text"], serde_json::json!("Count"));
        assert_eq!(text["tag"], serde_json::Value::Null);
    }

    /// `LAYOUT`: плоское дерево с rect, children по индексу, tag из VDOM.
    #[test]
    fn serialize_layout_shape() {
        let tree = sample_tree();
        let layout = Layout::build(&tree, (400.0, 300.0), &StubMetrics);
        let v = serialize_layout(&layout, Some(&tree));
        assert_eq!(v["key"], "0");
        assert_eq!(v["tag"], serde_json::json!("window"));
        // Корень занимает весь viewport.
        assert_eq!(v["w"], 400.0);
        assert_eq!(v["h"], 300.0);
        assert!(v["x"].is_number());
        assert!(v["y"].is_number());
        assert!(!v["children"].as_array().unwrap().is_empty());
    }

    /// `FIND` по тегу + тексту кнопки: возвращает key и rect.
    #[test]
    fn find_element_by_tag_and_text() {
        let tree = sample_tree();
        let layout = Layout::build(&tree, (400.0, 300.0), &StubMetrics);
        let res = find_element(&tree, Some(&layout), r#"{"tag":"button","text":"+1"}"#)
            .expect("button found");
        assert_eq!(res["key"], "0.0.0");
        assert_eq!(res["tag"], serde_json::json!("button"));
        assert!(res["rect"]["w"].is_number());
    }

    /// `FIND` по атрибуту.
    #[test]
    fn find_element_by_attr() {
        let mut tree = sample_tree();
        // Добавляем атрибут на кнопку.
        if let Some(div) = tree.children.get_mut(0)
            && let Some(btn) = div.children.get_mut(0)
        {
            btn.attrs.push(("class", "btn".to_owned()));
        }
        let res = find_element(&tree, None, r#"{"tag":"button","attr":{"class":"btn"}}"#)
            .expect("found by attr");
        assert_eq!(res["key"], "0.0.0");
    }

    /// `FIND` без совпадения → `None`; невалидный JSON → `None`.
    #[test]
    fn find_element_no_match_and_bad_json() {
        let tree = sample_tree();
        assert!(
            find_element(&tree, None, r#"{"tag":"input"}"#).is_none(),
            "no <input> in tree"
        );
        assert!(
            find_element(&tree, None, "not-json").is_none(),
            "bad json -> None"
        );
    }

    /// `SHOT`: выход начинается с `base64:` и декодируется в PNG-заголовок.
    #[cfg(feature = "devtools")]
    #[test]
    fn encode_screenshot_is_png_base64() {
        // 2x1 RGBA8.
        let buf: Vec<u8> = vec![255, 0, 0, 255, 0, 255, 0, 255];
        let out = encode_screenshot(&buf, 2, 1);
        let b64 = out.strip_prefix("base64:").expect("base64: prefix");
        use base64::Engine as _;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .expect("decodes");
        // PNG-магический байт-паттерн.
        assert_eq!(
            &bytes[0..8],
            &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]
        );
    }
}
