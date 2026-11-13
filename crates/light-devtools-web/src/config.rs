//! ТЗ 0.18.0: конфигурация Web UI DevTools из `light.config.json`.
//!
//! Формат файла (см. [`light_fw_cli_core::init::LIGHT_CONFIG`]):
//!
//! ```json
//! {
//!   "dev": {
//!     "devtools": { "enabled": true, "port": 9229 },
//!     "log_level": "debug"
//!   },
//!   "prod": { "devtools": { "enabled": false }, "log_level": "error" }
//! }
//! ```
//!
//! Для Web UI читаем опциональный раздел **`dev.devtools.web`**:
//!
//! ```json
//! { "dev": { "devtools": { "web": { "enabled": true, "port": 9228 } } } }
//! ```
//!
//! Если раздела `web` нет (или `light.config.json` не читается) — включаем
//! Web UI по умолчанию на порту [`DEFAULT_WEB_PORT`] (9228).

use serde_json::Value;

/// Порт Web UI DevTools по умолчанию.
pub const DEFAULT_WEB_PORT: u16 = 9228;

/// Настройки встроенного HTTP-сервера (Web UI DevTools).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebConfig {
    /// Запускать ли web-сервер при `light dev`.
    pub enabled: bool,
    /// Порт, на котором слушает web-сервер.
    pub port: u16,
}

impl Default for WebConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            port: DEFAULT_WEB_PORT,
        }
    }
}

/// Чистый разбор JSON `light.config.json` в [`WebConfig`].
///
/// Правила:
/// - раздел `dev.devtools.web` отсутствует → `enabled = true`, `port = 9228`;
/// - `web.port` не число (или вне диапазона, или 0) → дефолтный порт;
/// - `dev` / `devtools` не объект → дефолты;
/// - невалидный JSON → дефолты (не ломаем `light dev`).
///
/// # Примеры
///
/// ```
/// use light_fw_devtools_web::config::{parse, DEFAULT_WEB_PORT};
///
/// // Полный раздел.
/// let cfg = parse(r#"{"dev":{"devtools":{"web":{"enabled":false,"port":9300}}}}"#);
/// assert!(!cfg.enabled);
/// assert_eq!(cfg.port, 9300);
///
/// // Раздел отсутствует — дефолты.
/// let cfg = parse(r#"{}"#);
/// assert!(cfg.enabled);
/// assert_eq!(cfg.port, DEFAULT_WEB_PORT);
///
/// // Нечитаемый JSON — тоже дефолты (не ломаем `light dev`).
/// let cfg = parse("не json");
/// assert!(cfg.enabled);
/// assert_eq!(cfg.port, DEFAULT_WEB_PORT);
/// ```
pub fn parse(json: &str) -> WebConfig {
    let mut cfg = WebConfig::default();
    let Ok(v) = serde_json::from_str::<Value>(json) else {
        return cfg;
    };
    // Путь: `dev` → `devtools` → `web`.
    let Some(dev) = v.get("dev").filter(|d| d.is_object()) else {
        return cfg;
    };
    let Some(dt) = dev.get("devtools").filter(|d| d.is_object()) else {
        return cfg;
    };
    let Some(web) = dt.get("web").filter(|w| w.is_object()) else {
        return cfg;
    };
    if let Some(enabled) = web.get("enabled").and_then(|e| e.as_bool()) {
        cfg.enabled = enabled;
    }
    if let Some(port) = web
        .get("port")
        .and_then(|p| p.as_u64())
        .and_then(|p| u16::try_from(p).ok())
        .filter(|p| *p != 0)
    {
        cfg.port = port;
    }
    cfg
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_section_is_respected() {
        let cfg = parse(r#"{"dev":{"devtools":{"web":{"enabled":false,"port":9300}}}}"#);
        assert!(!cfg.enabled);
        assert_eq!(cfg.port, 9300);
    }

    #[test]
    fn missing_section_gives_defaults() {
        let cfg = parse(r#"{"other":1}"#);
        assert!(cfg.enabled);
        assert_eq!(cfg.port, DEFAULT_WEB_PORT);
    }

    #[test]
    fn empty_object_gives_defaults() {
        let cfg = parse("{}");
        assert!(cfg.enabled);
        assert_eq!(cfg.port, DEFAULT_WEB_PORT);
    }

    #[test]
    fn invalid_json_gives_defaults() {
        let cfg = parse("это не json");
        assert!(cfg.enabled);
        assert_eq!(cfg.port, DEFAULT_WEB_PORT);
    }

    #[test]
    fn non_numeric_port_falls_back_to_default() {
        let cfg = parse(r#"{"dev":{"devtools":{"web":{"port":"ошибка"}}}}"#);
        assert!(cfg.enabled);
        assert_eq!(cfg.port, DEFAULT_WEB_PORT);
    }

    #[test]
    fn out_of_range_port_falls_back_to_default() {
        let cfg = parse(r#"{"dev":{"devtools":{"web":{"port":70000}}}}"#);
        assert_eq!(cfg.port, DEFAULT_WEB_PORT);
    }

    #[test]
    fn zero_port_falls_back_to_default() {
        let cfg = parse(r#"{"dev":{"devtools":{"web":{"port":0}}}}"#);
        assert_eq!(cfg.port, DEFAULT_WEB_PORT);
    }

    #[test]
    fn partial_section_keeps_other_defaults() {
        let cfg = parse(r#"{"dev":{"devtools":{"web":{"port":9400}}}}"#);
        assert!(cfg.enabled);
        assert_eq!(cfg.port, 9400);
    }

    #[test]
    fn devtools_not_object_gives_defaults() {
        let cfg = parse(r#"{"dev":{"devtools":null}}"#);
        assert!(cfg.enabled);
        assert_eq!(cfg.port, DEFAULT_WEB_PORT);
    }

    #[test]
    fn realistic_light_config_format() {
        // Точный формат из init.rs (без web-секции) — всё равно дефолты.
        let cfg = parse(
            r#"{
                "dev": { "devtools": { "enabled": true, "port": 9229 }, "log_level": "debug" },
                "prod": { "devtools": { "enabled": false }, "log_level": "error" }
            }"#,
        );
        assert!(cfg.enabled);
        assert_eq!(cfg.port, DEFAULT_WEB_PORT);
    }

    #[test]
    fn realistic_with_web_section() {
        let cfg = parse(
            r#"{
                "dev": {
                    "devtools": {
                        "enabled": true,
                        "port": 9229,
                        "web": { "enabled": true, "port": 9500 }
                    },
                    "log_level": "debug"
                }
            }"#,
        );
        assert!(cfg.enabled);
        assert_eq!(cfg.port, 9500);
    }
}
