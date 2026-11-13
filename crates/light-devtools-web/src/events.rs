//! ТЗ 0.18.0: фоновая подписка на tree/state у TCP DevTools (fan-out в SSE).
//!
//! Один общий поток-опросчик каждые [`POLL`] секунд запрашивает `TREE` и
//! `SNAPSHOT` у `127.0.0.1:<port>` и рассылает события всем подписчикам
//! `/api/events`. Если TCP-сервер недоступен — рассылается событие с ошибкой
//! `{"kind":"tree","data":{"error":"devtools not running"}}`; браузерский
//! `EventSource` сам переключится на повторное подключение, а фронтенд покажет
//! бейдж «Ожидание приложения…».

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// Период опроса TCP DevTools для SSE-рассылки.
pub const POLL: Duration = Duration::from_millis(500);

type Subscriber = Box<dyn FnMut(&str) + Send>;

struct HubInner {
    subs: Vec<(u64, Subscriber)>,
}

/// Глобальный хаб подписчиков (одна инстанция на процесс).
static HUB: Mutex<HubInner> = Mutex::new(HubInner { subs: Vec::new() });

/// Счётчик токенов подписок.
static COUNTER: AtomicU64 = AtomicU64::new(1);

/// Подписывает callback на получение строк событий (SSE-фреймы).
/// Возвращает токен отмены.
pub fn subscribe<F>(cb: F) -> u64
where
    F: FnMut(&str) + Send + 'static,
{
    let id = next_id();
    let mut hub = lock_hub();
    hub.subs.push((id, Box::new(cb)));
    id
}

/// Отменяет подписку по токену (извлекает её из хаба).
pub fn unsubscribe(id: u64) {
    let mut hub = lock_hub();
    hub.subs.retain(|(sid, _)| *sid != id);
}

fn next_id() -> u64 {
    COUNTER.fetch_add(1, Ordering::Relaxed)
}

fn lock_hub() -> std::sync::MutexGuard<'static, HubInner> {
    HUB.lock().unwrap_or_else(|e| e.into_inner())
}

/// Рассылает готовый SSE-фрейм (`data: …\n\n`) всем подписчикам.
pub fn broadcast(frame: &str) {
    let mut hub = lock_hub();
    for (_, sub) in hub.subs.iter_mut() {
        sub(frame);
    }
}

/// Запускает фоновый поток-опросчик. `tcp_port` — порт TCP DevTools
/// (`0` → не опрашивать, фреймы с ошибкой «not running» не рассылаются).
/// Возвращает [`thread::JoinHandle`] и токен, по которому можно остановить
/// поллер (иначе он живёт до выхода процесса).
pub fn spawn_poller(tcp_port: u16) -> (thread::JoinHandle<()>, Arc<AtomicBool>) {
    let stop = Arc::new(AtomicBool::new(false));
    let stop_clone = stop.clone();
    let handle = thread::Builder::new()
        .name("devtools-web-events".into())
        .spawn(move || {
            if tcp_port == 0 {
                // Без порта — поллер просто спит, пока не попросят остановиться.
                while !stop_clone.load(Ordering::Relaxed) {
                    thread::sleep(POLL);
                }
                return;
            }
            let port = crate::proxy::resolve_port(Some(tcp_port));
            loop {
                if stop_clone.load(Ordering::Relaxed) {
                    return;
                }
                poll_once(port);
                thread::sleep(POLL);
            }
        })
        .expect("spawn events poller");
    (handle, stop)
}

/// Один тик: запрашиваем TREE и SNAPSHOT, рассылаем событиям.
fn poll_once(port: u16) {
    use light_fw_cli_core::devtools::{DevtoolsError, DevtoolsRequest, send_request};
    let fetch = |cmd: &str| -> Result<String, DevtoolsError> {
        send_request(
            port,
            &DevtoolsRequest {
                cmd: cmd.to_owned(),
                body: None,
                save_path: None,
                json: false,
            },
        )
    };
    match fetch("tree") {
        Ok(raw) => {
            let data = parse_json_or_raw(&raw);
            broadcast(&format!(
                "data: {}\n\n",
                serde_json::json!({ "kind": "tree", "data": data })
            ));
        }
        Err(DevtoolsError::NotRunning) => {
            broadcast(&format!(
                "data: {}\n\n",
                serde_json::json!({ "kind": "tree", "data": { "error": "devtools not running" } })
            ));
        }
        Err(other) => {
            broadcast(&format!(
                "data: {}\n\n",
                serde_json::json!({ "kind": "tree", "data": { "error": format!("{other}") } })
            ));
        }
    }
    match fetch("state") {
        Ok(raw) => {
            let data = parse_json_or_raw(&raw);
            broadcast(&format!(
                "data: {}\n\n",
                serde_json::json!({ "kind": "state", "data": data })
            ));
        }
        Err(DevtoolsError::NotRunning) => {
            broadcast(&format!(
                "data: {}\n\n",
                serde_json::json!({ "kind": "state", "data": { "error": "devtools not running" } })
            ));
        }
        Err(other) => {
            broadcast(&format!(
                "data: {}\n\n",
                serde_json::json!({ "kind": "state", "data": { "error": format!("{other}") } })
            ));
        }
    }
}

/// Парсит JSON; при ошибке возвращает plain-строку (для "no ui", "not found" и т.п.).
fn parse_json_or_raw(raw: &str) -> serde_json::Value {
    serde_json::from_str::<serde_json::Value>(raw)
        .unwrap_or(serde_json::Value::String(raw.to_owned()))
}
