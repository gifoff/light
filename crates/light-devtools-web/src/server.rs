//! ТЗ 0.18.0: HTTP-сервер Web UI DevTools (tiny_http, без tokio/axum).
//!
//! Слушает только на `127.0.0.1`. Маршруты:
//! - `GET /` — `index.html`;
//! - `GET /assets/app.js`, `GET /assets/style.css` — вшитые через `include_str!`;
//! - `GET /api/tree` — прокси `TREE`;
//! - `GET /api/state` — прокси `SNAPSHOT`;
//! - `POST /api/command` — прокси произвольной команды;
//! - `GET /api/events` — SSE-поток `{"kind":"tree"|"state","data":{...}}` каждые 500 мс.
//!
//! Если TCP DevTools (127.0.0.1:9229) не запущен — `/api/*` и `/api/events`
//! возвращают `{"error":"devtools not running"}` (HTTP 200); сервер продолжает
//! работать, а фронтенд сам переподключится.

use std::thread;

use tiny_http::{Header, Method, Server, StatusCode};

use crate::config::WebConfig;
use crate::events;
use crate::proxy;

/// Порт TCP DevTools по умолчанию (совпадает с `light-fw-cli-core`).
pub const DEVTOOLS_TCP_PORT: u16 = 9229;

// Вшитые статические ассеты.
const INDEX_HTML: &str = include_str!("../assets/index.html");
const APP_JS: &str = include_str!("../assets/app.js");
const STYLE_CSS: &str = include_str!("../assets/style.css");

/// Параметры запуска web-сервера.
#[derive(Debug, Clone)]
pub struct ServeOptions {
    /// Настройки из light.config.json (порт/вкл-выкл).
    pub config: WebConfig,
    /// Порт TCP DevTools (по умолчанию 9229; `0` — SSE-поллер не опрашивает).
    pub devtools_port: u16,
}

impl Default for ServeOptions {
    fn default() -> Self {
        Self {
            config: WebConfig::default(),
            devtools_port: DEVTOOLS_TCP_PORT,
        }
    }
}

/// Ответ чистого роутера (тестируется без живого сервера).
///
/// Имя `RouterResponse` чтобы не конфликтовать с [`tiny_http::Response`].
pub struct RouterResponse {
    /// HTTP-статус.
    pub status: u16,
    /// Content-Type ответа.
    pub content_type: String,
    /// Тело ответа (UTF-8).
    pub body: String,
}

impl RouterResponse {
    /// Простой JSON-ответ 200.
    pub fn json(body: &str) -> Self {
        Self {
            status: 200,
            content_type: "application/json; charset=utf-8".into(),
            body: body.to_owned(),
        }
    }

    /// Простой текст (HTML/CSS/JS).
    fn text(status: u16, content_type: &str, body: &str) -> Self {
        Self {
            status,
            content_type: content_type.to_owned(),
            body: body.to_owned(),
        }
    }
}

/// Разбирает путь запроса и отдаёт ответ, кроме SSE (`/api/events` обрабатывается отдельно).
///
/// # Примеры
///
/// ```
/// use light_fw_devtools_web::server::route;
///
/// // `GET /` → HTML 200.
/// let r = route("/", "GET", "");
/// assert_eq!(r.status, 200);
/// assert!(r.body.starts_with("<!doctype html>"), "{}", r.body);
///
/// // Неизвестная команда → `ok:false` (не паника, не 404).
/// let r = route("/api/command", "POST", r#"{"cmd":"unknown"}"#);
/// assert_eq!(r.status, 200);
/// assert!(r.body.contains("\"ok\":false"), "{}", r.body);
/// ```
pub fn route(path: &str, method: &str, body: &str) -> RouterResponse {
    match (method, path) {
        ("GET", "/") => RouterResponse::text(200, "text/html; charset=utf-8", INDEX_HTML),
        ("GET", "/assets/app.js") => {
            RouterResponse::text(200, "application/javascript; charset=utf-8", APP_JS)
        }
        ("GET", "/assets/style.css") => {
            RouterResponse::text(200, "text/css; charset=utf-8", STYLE_CSS)
        }
        ("GET", "/api/tree") => RouterResponse::json(&proxy::proxy_get(DEVTOOLS_TCP_PORT, "tree")),
        ("GET", "/api/state") => {
            RouterResponse::json(&proxy::proxy_get(DEVTOOLS_TCP_PORT, "state"))
        }
        ("POST", "/api/command") => {
            RouterResponse::json(&proxy::proxy_command(DEVTOOLS_TCP_PORT, body))
        }
        _ => RouterResponse::json(
            &serde_json::json!({
                "ok": false,
                "error": format!("not found: {method} {path}")
            })
            .to_string(),
        ),
    }
}

/// Строит список кандидатов портов: `base` либо `base..=base+10`.
/// - `None` → пустой вектор;
/// - `Some(0)` → ephemeral, один кандидат `[0]`;
/// - `Some(b)` → `[b, b+1, …, b+10]` (saturating add, max u16::MAX).
fn port_candidates(base: Option<u16>) -> Vec<u16> {
    match base {
        Some(0) => vec![0],
        Some(b) => (0..=10).map(|d| b.saturating_add(d)).collect(),
        None => vec![],
    }
}

/// Запускает web-сервер в текущем потоке (блокирующе). Возвращает реальный порт,
/// если бинд на порту `0` (для тестов), либо выбранный после fallback'а.
///
/// `shutdown` — общий флаг остановки; когда он становится `true`, главный
/// цикл вызывает `server.unblock()` и завершается. Внешний код (см.
/// [`WebServer`]) устанавливает флаг через свой `stop()` / `Drop`.
///
/// При ошибке бинда на всех кандидатах печатает warning и возвращает `None`.
fn serve_blocking_inner(
    opts: &ServeOptions,
    port_override: Option<u16>,
    shutdown: &std::sync::atomic::AtomicBool,
) -> Option<u16> {
    if !opts.config.enabled {
        eprintln!("light dev: web ui отключён в light.config.json (devtools.web.enabled=false)");
        return None;
    }

    // Базовый порт: override (тесты/порт `0`) либо конфиг (+ fallback +1..+10).
    let base: Option<u16> = port_override.or(Some(opts.config.port));
    let candidates = port_candidates(base);

    let mut last_err: Option<String> = None;
    let mut bound: Option<Server> = None;
    for p in &candidates {
        match Server::http(format!("127.0.0.1:{p}")) {
            Ok(s) => {
                bound = Some(s);
                break;
            }
            Err(e) => {
                last_err = Some(e.to_string());
            }
        }
    }
    let server = match bound {
        Some(s) => s,
        None => {
            eprintln!(
                "light dev: web dev tools не запущен — все порты {:?} заняты{}",
                candidates,
                last_err
                    .as_deref()
                    .map(|m| format!(" ({m})"))
                    .unwrap_or_default()
            );
            return None;
        }
    };

    // Реальный порт (важен для ephemeral `0`).
    let real_port = match server.server_addr().to_ip() {
        Some(a) => a.port(),
        None => 0,
    };
    println!("Web DevTools: http://127.0.0.1:{real_port}");

    // Фоновый poller (SSE) — отдельный тред. `0` → поллер спит (без опроса).
    let (poller, poller_stop) = events::spawn_poller(opts.devtools_port);

    // Главный цикл: принимаем запросы с коротким таймаутом, чтобы быстро
    // реагировать на shutdown. `recv_timeout` возвращает `Ok(None)` если
    // таймаут истёк без запросов.
    'outer: loop {
        if shutdown.load(std::sync::atomic::Ordering::Relaxed) {
            server.unblock();
            break 'outer;
        }
        match server.recv_timeout(std::time::Duration::from_millis(200)) {
            Ok(Some(req)) => handle_request(req, shutdown),
            Ok(None) => continue,    // таймаут — проверяем shutdown и повторяем
            Err(_e) => break 'outer, // ошибка (в т.ч. close после unblock)
        }
    }

    // Останавливаем poller и ждём.
    poller_stop.store(true, std::sync::atomic::Ordering::Relaxed);
    let _ = poller.join();
    // `server` дропнется здесь — это тоже вызывает unblock + close socket.
    Some(real_port)
}

/// Обработка одного запроса. Для SSE — держим соединение открытым;
/// иначе — синхронный ответ.
fn handle_request(mut req: tiny_http::Request, shutdown: &std::sync::atomic::AtomicBool) {
    let url = req.url();
    // `url()` уже содержит ведущий `/`.
    let path = if url.starts_with('/') {
        url.to_owned()
    } else {
        format!("/{url}")
    };
    let method = match req.method() {
        Method::Get => "GET",
        Method::Post => "POST",
        _ => "OTHER",
    };

    if path == "/api/events" && method == "GET" {
        handle_sse(req, shutdown);
        return;
    }

    // Тело (для POST /api/command).
    let mut body_buf = String::new();
    if matches!(method, "POST" | "PUT" | "PATCH") {
        let _ = req.as_reader().read_to_string(&mut body_buf);
    }
    let resp = route(&path, method, &body_buf);
    let sc = StatusCode::from(resp.status);
    let ct = Header::from_bytes("Content-Type", resp.content_type.clone()).unwrap_or_else(|_| {
        Header::from_bytes("Content-Type", "application/octet-stream")
            .unwrap_or_else(|_| Header::from_bytes("Content-Length", "0").unwrap())
    });
    let response = tiny_http::Response::from_data(resp.body.into_bytes());
    let response = response.with_status_code(sc);
    let response = response.with_header(ct);
    let _ = req.respond(response);
}

/// SSE: держим соединение открытым до тех пор, пока клиент не отключится.
///
/// Реализация через [`tiny_http::Request::into_writer`] — прямой доступ к
/// сокету, где мы вручную пишем HTTP-заголовок с `Transfer-Encoding: chunked`
/// и последующие чанки. Это надёжнее, чем `req.respond(Response)` со
/// streaming-ридером (который в tiny_http 0.12 буферизует до первого
/// `read()`, а на Windows может не флешить).
///
/// При подключении сразу отправляется приветственный фрейм, чтобы клиент
/// (и тест) получили подтверждение, что соединение установлено.
fn handle_sse(req: tiny_http::Request, shutdown: &std::sync::atomic::AtomicBool) {
    use std::io::Write;
    use std::sync::mpsc;
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    let tx_for_sub = tx.clone();
    let token = events::subscribe(move |frame: &str| {
        let _ = tx_for_sub.send(frame.as_bytes().to_vec());
    });
    // Приветственный фрейм — подтверждает, что SSE-канал жив.
    let _ = tx.send(b"data: {\"kind\":\"hello\"}\n\n".to_vec());
    let mut writer = req.into_writer();
    let hdrs = b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream; charset=utf-8\r\nCache-Control: no-cache\r\nX-Accel-Buffering: no\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n";
    if writer.write_all(hdrs).is_err() || writer.flush().is_err() {
        return;
    }
    // Читаем фреймы из канала и отправляем их как HTTP chunked frames.
    loop {
        if shutdown.load(std::sync::atomic::Ordering::Relaxed) {
            break;
        }
        match rx.recv_timeout(std::time::Duration::from_millis(500)) {
            Ok(bytes) => {
                let hex_len = format!("{:x}\r\n", bytes.len());
                let mut chunk = Vec::with_capacity(hex_len.len() + bytes.len() + 2);
                chunk.extend_from_slice(hex_len.as_bytes());
                chunk.extend_from_slice(&bytes);
                chunk.extend_from_slice(b"\r\n");
                if writer.write_all(&chunk).is_err() {
                    break;
                }
                if writer.flush().is_err() {
                    break;
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    // Записываем zero-length chunk (конец chunked body).
    let _ = writer.write_all(b"0\r\n\r\n");
    let _ = writer.flush();
    events::unsubscribe(token);
}

/// Хэндл на web-сервер. Обёртка над потоком-обработчиком + общим флагом
/// остановки.
///
/// `WebServer` реализует `Drop` — при выходе из области видимости (например,
/// в `Ev::Exited` в `light dev`) автоматически:
/// 1. устанавливает `shutdown` флаг,
/// 2. `join()`-ит поток-обработчик (он сам зовёт `unblock()` и завершается).
///
/// Это безопасно: основной поток процесса не может завершиться, пока жив
/// не-daemon тред, а `Drop` вызывается при выхождении `WebServer` из scope.
pub struct WebServer {
    /// Тред-обработчик; `take()` + `join()` — при остановке.
    pub handle: std::sync::Mutex<Option<thread::JoinHandle<Option<u16>>>>,
    /// Общий флаг остановки (общий между обработчиком и внешним кодом).
    pub shutdown: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl Drop for WebServer {
    fn drop(&mut self) {
        self.shutdown
            .store(true, std::sync::atomic::Ordering::Relaxed);
        let h = self.handle.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(h) = h {
            // Ждём с таймаутом: SSE-клиент может держать соединение открытым,
            // и `join()` заблокировался бы навсегда.
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
            loop {
                if h.is_finished() {
                    break;
                }
                if std::time::Instant::now() > deadline {
                    eprintln!("light dev: web server thread didn't finish in 3s (SSE client?)");
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            let _ = h.join();
        }
    }
}

impl WebServer {
    /// Явно останавливает сервер: устанавливает `shutdown`, ждёт потока.
    /// Возвращает реальный порт (если сервер успел стартовать).
    pub fn stop(self) -> Option<u16> {
        self.shutdown
            .store(true, std::sync::atomic::Ordering::Relaxed);
        let h = self.handle.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(h) = h {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
            loop {
                if h.is_finished() {
                    break;
                }
                if std::time::Instant::now() > deadline {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            return h.join().ok().flatten();
        }
        None
    }
}

/// Запускает web-сервер в отдельном треде (не-блокирующе). Возвращает
/// [`WebServer`], который при `drop` автоматически останавливает сервер.
///
/// `port_override: Some(0)` — ephemeral (для тестов); `Some(p)` — конкретный
/// порт без fallback'а; `None` — порт из конфига + fallback +1..+10.
pub fn serve(opts: ServeOptions, port_override: Option<u16>) -> WebServer {
    let shutdown: std::sync::Arc<std::sync::atomic::AtomicBool> =
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let shutdown_for_thread = shutdown.clone();
    let handle = thread::Builder::new()
        .name("devtools-web".into())
        .spawn(move || serve_blocking_inner(&opts, port_override, &shutdown_for_thread))
        .expect("spawn web thread");
    WebServer {
        handle: std::sync::Mutex::new(Some(handle)),
        shutdown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_returns_html_200() {
        let r = route("/", "GET", "");
        assert_eq!(r.status, 200);
        assert!(r.body.starts_with("<!doctype html>"), "{}", r.body);
        assert!(r.content_type.starts_with("text/html"));
    }

    #[test]
    fn assets_app_js_200() {
        let r = route("/assets/app.js", "GET", "");
        assert_eq!(r.status, 200);
        assert!(r.content_type.contains("javascript"));
        // app.js содержит функцию refresh (минимальный smoke-check).
        assert!(
            r.body.contains("refresh"),
            "app.js: {}",
            &r.body[..r.body.len().min(200)]
        );
    }

    #[test]
    fn assets_style_css_200() {
        let r = route("/assets/style.css", "GET", "");
        assert_eq!(r.status, 200);
        assert!(r.content_type.contains("css"));
        assert!(
            r.body.contains(".tree"),
            "style.css должен содержать правило .tree"
        );
    }

    #[test]
    fn unknown_route_json_error() {
        let r = route("/api/nope", "GET", "");
        assert_eq!(r.status, 200);
        assert!(r.body.contains("not found") || r.body.contains("error"));
    }

    #[test]
    fn api_tree_contract_has_ok_and_error_or_result() {
        // Контракт: каждый JSON-ответ /api/tree содержит `ok` и либо `result`,
        // либо `error`. Поле `data` в HTTP-контракте не используется.
        let r = route("/api/tree", "GET", "");
        assert_eq!(r.status, 200);
        let body_preview: String = r.body.chars().take(120).collect();
        let v: serde_json::Value = serde_json::from_str(&r.body)
            .unwrap_or_else(|_| panic!("invalid JSON in /api/tree response: {body_preview}"));
        assert!(v.get("ok").is_some(), "отсутствует поле `ok`: {}", r.body);
        assert!(
            !v.get("data").is_some(),
            "поле `data` не должно быть в HTTP-ответе: {}",
            r.body
        );
        assert!(
            v.get("result").is_some() || v.get("error").is_some(),
            "ожидался `result` или `error`: {}",
            r.body
        );
    }

    #[test]
    fn api_command_unknown_cmd_ok_false() {
        let r = route("/api/command", "POST", r#"{"cmd":"unknown","args":null}"#);
        assert_eq!(r.status, 200);
        // Без запущенного приложения — "devtools not running".
        // С запущенным — "unknown command". В обоих случаях ok:false или error.
        // Благодаря клиентскому allowlist'у неизвестная команда отклоняется
        // БЕЗ обращения к TCP — всегда ok:false (hermetic).
        assert!(r.body.contains("\"ok\":false"), "{}", r.body);
        assert!(r.body.contains("unknown command: unknown"), "{}", r.body);
    }

    #[test]
    fn port_candidates_range() {
        let c = port_candidates(Some(9228));
        assert_eq!(c.len(), 11);
        assert_eq!(c[0], 9228);
        assert_eq!(c[10], 9238);
    }
}
