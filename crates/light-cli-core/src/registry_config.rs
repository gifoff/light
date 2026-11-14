//! Конфигурация реестра (`registry` в `light.config.json`).
//!
//! Этап 0 Plugin API: `light search` находит компоненты в **конфигурируемом**
//! реестре (по умолчанию crates.io). URL не хардкодится в коде — все пути
//! приходят из [`RegistryConfig`], который читается из секции `registry` файла
//! `light.config.json` в корне проекта. Если файл отсутствует или битый —
//! используются дефолты (crates.io) с предупреждением.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Дефолтный URL API реестра (crates.io).
pub const DEFAULT_REGISTRY_URL: &str = "https://crates.io";

/// Дефолтный URL загрузки артефактов (static.crates.io).
pub const DEFAULT_DOWNLOAD_URL: &str = "https://static.crates.io";

/// Дефолтный URL документации (docs.rs).
pub const DEFAULT_DOCS_URL: &str = "https://docs.rs";

/// Имя конфигурационного файла проекта.
const CONFIG_FILE: &str = "light.config.json";

/// Секция `registry` из `light.config.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistrySection {
    /// Базовый URL API (поиск, owners).
    #[serde(default = "default_url")]
    pub url: String,
    /// Базовый URL загрузки `.crate`-артефактов.
    #[serde(default = "default_download_url")]
    pub download_url: String,
    /// Базовый URL документации (docs.rs).
    #[serde(default = "default_docs_url")]
    pub docs_url: String,
}

fn default_url() -> String {
    DEFAULT_REGISTRY_URL.to_string()
}

fn default_download_url() -> String {
    DEFAULT_DOWNLOAD_URL.to_string()
}

fn default_docs_url() -> String {
    DEFAULT_DOCS_URL.to_string()
}

impl Default for RegistrySection {
    fn default() -> Self {
        Self {
            url: default_url(),
            download_url: default_download_url(),
            docs_url: default_docs_url(),
        }
    }
}

/// Конфигурация реестра: где искать, откуда скачивать, где доки.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryConfig {
    /// Базовый URL API (поиск, owners).
    pub url: String,
    /// Базовый URL загрузки `.crate`-артефактов.
    pub download_url: String,
    /// Базовый URL документации (docs.rs).
    pub docs_url: String,
    /// Источниковый файл, из которого была прочитана конфигурация.
    pub source: ConfigSource,
}

impl RegistryConfig {
    /// Создаёт конфигурацию из секции `registry`.
    pub fn from_section(section: RegistrySection, source: ConfigSource) -> Self {
        Self {
            url: section.url,
            download_url: section.download_url,
            docs_url: section.docs_url,
            source,
        }
    }

    /// Дефолтная конфигурация (crates.io), без какого-либо файла.
    pub fn default_registry() -> Self {
        let section = RegistrySection::default();
        let url = section.url.clone();
        Self {
            url,
            download_url: section.download_url,
            docs_url: section.docs_url,
            source: ConfigSource::Builtin,
        }
    }

    /// Читает конфигурацию из `light.config.json` в `project_root`.
    ///
    /// Возвращает `(config, warning)`: `warning` — `Some(String)` если файл
    /// отсутствовал или содержал некорректный JSON (в обоих случаях
    /// возвращается дефолт crates.io).
    pub fn load(project_root: &Path) -> (Self, Option<String>) {
        let path = project_root.join(CONFIG_FILE);
        if !path.exists() {
            return (
                Self::default_registry(),
                Some(format!(
                    "файл `{}` не найден — используется реестр по умолчанию",
                    CONFIG_FILE
                )),
            );
        }
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) => {
                return (
                    Self::default_registry(),
                    Some(format!(
                        "не удалось прочитать `{}`: {e} — используется реестр по умолчанию",
                        CONFIG_FILE
                    )),
                );
            }
        };
        // Разбираем как Value: файл содержит ещё `dev`/`prod`, и мы берём
        // только `registry`. Нехватка полей → `#[serde(default)]` подставит
        // дефолты; неверный тип (число вместо строки) → ошибка + дефолт.
        let value: serde_json::Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(e) => {
                return (
                    Self::default_registry(),
                    Some(format!(
                        "`{}` содержит некорректный JSON ({e}) — используется реестр по умолчанию",
                        CONFIG_FILE
                    )),
                );
            }
        };
        let registry_value = value.get("registry");
        match registry_value {
            None => (
                Self::default_registry(),
                Some(format!(
                    "секция `registry` отсутствует в `{}` — используется реестр по умолчанию",
                    CONFIG_FILE
                )),
            ),
            Some(r) => match serde_json::from_value::<RegistrySection>(r.clone()) {
                Ok(section) => (
                    Self {
                        url: section.url,
                        download_url: section.download_url,
                        docs_url: section.docs_url,
                        source: ConfigSource::File(path.clone()),
                    },
                    None,
                ),
                Err(e) => (
                    Self::default_registry(),
                    Some(format!(
                        "секция `registry` в `{}` некорректна ({e}) — используется реестр по умолчанию",
                        CONFIG_FILE
                    )),
                ),
            },
        }
    }

    /// Строит URL поиска: `<url>/api/v1/crates?q=<query>&per_page=<n>&sort=relevance`.
    pub fn search_url(&self, query: &str, limit: usize) -> String {
        format!(
            "{}/api/v1/crates?q={}&per_page={limit}&sort=relevance",
            self.url.trim_end_matches('/'),
            urlencoding(query)
        )
    }

    /// Строит URL владельцев крейта: `<url>/api/v1/crates/<name>/owners`.
    pub fn owners_url(&self, name: &str) -> String {
        format!(
            "{}/api/v1/crates/{}/owners",
            self.url.trim_end_matches('/'),
            name
        )
    }

    /// Строит URL загрузки `.crate`: `<download_url>/<name>/<version>/download`.
    pub fn download_url_for(&self, name: &str, version: &str) -> String {
        format!(
            "{}/{name}/{version}/download",
            self.download_url.trim_end_matches('/')
        )
    }

    /// Строит URL страницы крейта: `<url>/crates/<name>`.
    pub fn crate_page(&self, name: &str) -> String {
        format!("{}/crates/{name}", self.url.trim_end_matches('/'))
    }

    /// Строит URL документации: `<docs_url>/<name>/<version>`.
    pub fn docs_url_for(&self, name: &str, version: &str) -> String {
        format!("{}/{name}/{version}", self.docs_url.trim_end_matches('/'))
    }
}

/// Откуда взялась конфигурация.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigSource {
    /// Встроенный дефолт (crates.io), файл не читался.
    Builtin,
    /// Прочитано из конкретного файла `light.config.json`.
    File(PathBuf),
}

/// Минимальное URL-кодирование для `q=` параметра (пробел → `+`, спецсимволы
/// → `%XX`). Покрывает латиницу, цифры, `-_.~` без кодирования; остальное —
/// побайтово.
fn urlencoding(value: &str) -> String {
    let mut out = String::with_capacity(value.len() * 2);
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

// ────────────────────────────── Тесты ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_config(root: &Path, body: &str) {
        std::fs::create_dir_all(root).expect("mkdir");
        let mut f = std::fs::File::create(root.join(CONFIG_FILE)).expect("open");
        f.write_all(body.as_bytes()).expect("write");
    }

    #[test]
    fn load_missing_file_returns_default_with_warning() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (cfg, warning) = RegistryConfig::load(dir.path());
        assert_eq!(cfg.url, DEFAULT_REGISTRY_URL);
        assert_eq!(cfg.download_url, DEFAULT_DOWNLOAD_URL);
        assert_eq!(cfg.docs_url, DEFAULT_DOCS_URL);
        assert!(matches!(cfg.source, ConfigSource::Builtin));
        assert!(
            warning.is_some(),
            "должен быть warning при отсутствии файла"
        );
        assert!(warning.unwrap().contains("не найден"));
    }

    #[test]
    fn load_custom_urls_from_registry_section() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_config(
            dir.path(),
            r#"{
                "dev": {"log_level": "debug"},
                "prod": {"log_level": "error"},
                "registry": {
                    "url": "https://panamax.local",
                    "download_url": "https://dl.panamax.local",
                    "docs_url": "https://docs.panamax.local"
                }
            }"#,
        );
        let (cfg, warning) = RegistryConfig::load(dir.path());
        assert!(warning.is_none(), "warning не ожидается: {warning:?}");
        assert_eq!(cfg.url, "https://panamax.local");
        assert_eq!(cfg.download_url, "https://dl.panamax.local");
        assert_eq!(cfg.docs_url, "https://docs.panamax.local");
        assert!(
            matches!(cfg.source, ConfigSource::File(_)),
            "source должен быть File"
        );
    }

    #[test]
    fn load_broken_json_returns_default_with_warning() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_config(dir.path(), "{ это не json ");
        let (cfg, warning) = RegistryConfig::load(dir.path());
        assert_eq!(cfg.url, DEFAULT_REGISTRY_URL);
        assert!(
            matches!(cfg.source, ConfigSource::Builtin),
            "битый JSON → встроенный дефолт"
        );
        assert!(warning.is_some());
        assert!(warning.unwrap().contains("некорректный JSON"));
    }

    #[test]
    fn load_registry_missing_field_uses_default_for_that_field() {
        let dir = tempfile::tempdir().expect("tempdir");
        // Только url задан, download_url/docs_url опущены → дефолты.
        write_config(
            dir.path(),
            r#"{ "registry": { "url": "https://my-reg.local" } }"#,
        );
        let (cfg, warning) = RegistryConfig::load(dir.path());
        assert!(warning.is_none(), "warning не ожидается: {warning:?}");
        assert_eq!(cfg.url, "https://my-reg.local");
        assert_eq!(cfg.download_url, DEFAULT_DOWNLOAD_URL);
        assert_eq!(cfg.docs_url, DEFAULT_DOCS_URL);
    }

    #[test]
    fn load_no_registry_section_returns_default_with_warning() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_config(dir.path(), r#"{ "dev": {}, "prod": {} }"#);
        let (cfg, warning) = RegistryConfig::load(dir.path());
        assert_eq!(cfg.url, DEFAULT_REGISTRY_URL);
        assert!(
            matches!(cfg.source, ConfigSource::Builtin),
            "нет секции → Builtin"
        );
        assert!(warning.is_some());
        assert!(warning.unwrap().contains("отсутствует"));
    }

    #[test]
    fn load_wrong_type_in_registry_returns_default_with_warning() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_config(dir.path(), r#"{ "registry": 42 }"#);
        let (cfg, warning) = RegistryConfig::load(dir.path());
        assert_eq!(cfg.url, DEFAULT_REGISTRY_URL);
        assert!(
            matches!(cfg.source, ConfigSource::Builtin),
            "неверный тип → Builtin"
        );
        assert!(warning.is_some());
        assert!(warning.unwrap().contains("некорректна"));
    }

    #[test]
    fn build_search_url_encodes_query_and_appends_limit() {
        let cfg = RegistryConfig::default_registry();
        let url = cfg.search_url("date picker", 30);
        assert_eq!(
            url,
            "https://crates.io/api/v1/crates?q=date+picker&per_page=30&sort=relevance"
        );
    }

    #[test]
    fn build_search_url_handles_umlauts_and_unicode() {
        let cfg = RegistryConfig::default_registry();
        let url = cfg.search_url("café", 5);
        assert!(
            url.contains("q=caf%C3%A9"),
            "ожидается %C3%A9 для é, получили: {url}"
        );
        assert!(url.ends_with("&per_page=5&sort=relevance"));
    }

    #[test]
    fn build_owners_url() {
        let cfg = RegistryConfig::default_registry();
        assert_eq!(
            cfg.owners_url("light-widget-datepicker"),
            "https://crates.io/api/v1/crates/light-widget-datepicker/owners"
        );
    }

    #[test]
    fn build_download_url_uses_download_host() {
        let cfg = RegistryConfig::default_registry();
        assert_eq!(
            cfg.download_url_for("foo", "1.2.3"),
            "https://static.crates.io/foo/1.2.3/download"
        );
    }

    #[test]
    fn build_crate_page_and_docs_url() {
        let cfg = RegistryConfig::default_registry();
        assert_eq!(cfg.crate_page("foo"), "https://crates.io/crates/foo");
        assert_eq!(
            cfg.docs_url_for("foo", "1.2.3"),
            "https://docs.rs/foo/1.2.3"
        );
    }

    #[test]
    fn trailing_slash_is_normalized() {
        let section = RegistrySection {
            url: "https://reg.local/".into(),
            download_url: "https://dl.local/".into(),
            docs_url: "https://docs.local/".into(),
        };
        let cfg = RegistryConfig::from_section(section, ConfigSource::Builtin);
        assert_eq!(
            cfg.owners_url("x"),
            "https://reg.local/api/v1/crates/x/owners"
        );
        assert_eq!(
            cfg.download_url_for("x", "1.0.0"),
            "https://dl.local/x/1.0.0/download"
        );
    }

    #[test]
    fn urlencoding_keeps_safe_chars() {
        assert_eq!(urlencoding("abc-123.XY_~"), "abc-123.XY_~");
        assert_eq!(urlencoding("a b"), "a+b");
        assert_eq!(urlencoding("é"), "%C3%A9");
    }

    // Проверка, что секция сериализуется в ожидаемый JSON (для init).
    #[test]
    fn registry_section_serializes_to_expected_json() {
        let section = RegistrySection::default();
        let v: serde_json::Value = serde_json::to_value(&section).expect("serialize");
        assert_eq!(v["url"], DEFAULT_REGISTRY_URL);
        assert_eq!(v["download_url"], DEFAULT_DOWNLOAD_URL);
        assert_eq!(v["docs_url"], DEFAULT_DOCS_URL);
    }
}
