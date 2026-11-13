//! # Light DevTools · Web UI (ТЗ 0.18.0, v1)
//!
//! Встроенный HTTP-сервер для `light dev`: браузерный инспектор VDOM/State
//! + отправка команд (click/type/key/shot) на работающий TCP DevTools
//!   (`127.0.0.1:9229`). Реализован на [`tiny_http`] — **без tokio/axum**,
//!   только std::thread + SSE (Server-Sent Events).
//!
//! ## Публичный API
//!
//! ```no_run
//! use light_fw_devtools_web::{config, server};
//!
//! // Запустить сервер в отдельном треде (нужен запущенный `light dev` с
//! // включённым DevTools на 9229, иначе /api/* вернут "devtools not running").
//! let opts = server::ServeOptions {
//!     config: config::WebConfig { enabled: true, port: 9228 },
//!     devtools_port: 9229,
//! };
//! let web = server::serve(opts, None);
//! // ... позже, при остановке: web.stop() (установка shutdown-флага + join).
//! ```
//!
//! ## Ограничения v1
//!
//! - Только один процесс `light dev` (один TCP DevTools на 9229).
//! - Нет авторизации и TLS (слушатель привязан к 127.0.0.1).
//! - Нет WebSocket — только SSE + fetch.
//!
//! [`tiny_http`]: https://crates.io/crates/tiny_http

pub mod config;
pub mod events;
pub mod proxy;
pub mod server;

#[cfg(test)]
mod tests {
    use crate::config;
    use crate::server::route;

    /// ТЗ: `config::parse()` читает `dev.devtools.web.{enabled,port}` из JSON.
    #[test]
    fn config_parse_reads_web_section() {
        let json = r#"{
            "dev": {
                "devtools": {
                    "enabled": true,
                    "port": 9229,
                    "web": { "enabled": false, "port": 9300 }
                },
                "log_level": "debug"
            }
        }"#;
        let cfg = config::parse(json);
        assert!(!cfg.enabled, "enabled=false должен быть прочитан");
        assert_eq!(cfg.port, 9300);
    }

    /// ТЗ: `config::parse()` без секции → дефолты enabled=true, port=9228.
    #[test]
    fn config_parse_defaults_when_missing() {
        let cfg = config::parse(r#"{"dev":{"devtools":{"enabled":true,"port":9229}}}"#);
        assert!(cfg.enabled, "default: enabled=true");
        assert_eq!(cfg.port, 9228);
    }

    /// ТЗ: `server::route("/")` возвращает 200 с телом, начинающимся с `<!doctype html`.
    #[test]
    fn route_root_returns_html_200() {
        let r = route("/", "GET", "");
        assert_eq!(r.status, 200);
        assert!(r.body.starts_with("<!doctype html>"), "{}", r.body);
        assert!(r.content_type.starts_with("text/html"));
    }

    /// ТЗ: `server::route("/api/command")` с телом `{"cmd":"unknown"}` → 200 `ok:false`.
    ///
    /// Благодаря клиентскому allowlist'у неизвестная команда отклоняется
    /// БЕЗ обращения к TCP — всегда `ok:false` (hermetic).
    #[test]
    fn route_command_unknown_cmd_ok_false() {
        let r = route("/api/command", "POST", r#"{"cmd":"unknown","args":null}"#);
        assert_eq!(r.status, 200);
        assert!(
            r.body.contains("\"ok\":false"),
            "ожидался ok:false, получили: {}",
            r.body
        );
        assert!(
            r.body.contains("unknown command: unknown"),
            "ожидалась причина unknown command, получили: {}",
            r.body
        );
    }
}
