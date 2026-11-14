//! Интеграционные тесты `light search` через wiremock.
//!
//! Тестируют: ранжирование (официальные > имя-в-описании > загрузки),
//! prefix-фильтрацию, owners-проверку, download + validate.
//!
//! Запускаются только с фичей `network` (нужен reqwest для реальных
//! HTTP-запросов к wiremock-серверу):
//! ```bash
//! cargo test -p light-fw-cli-core --test search_integration --features network
//! ```

#![cfg(feature = "network")]

use light_fw_cli_core::fetch::{RegistryClient, RemoteRegistry};
use light_fw_cli_core::registry_config::RegistryConfig;
use light_fw_cli_core::search::{SearchResult, filter_by_prefix, rank, truncate_description};
use wiremock::matchers::{method, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Строит tar.gz архив с `Cargo.toml` внутри.
fn crate_bytes(cargo_toml: &str) -> Vec<u8> {
    use flate2::Compression;
    use flate2::write::GzEncoder;
    use std::io::Write;

    let mut builder = tar::Builder::new(Vec::new());
    let mut header = tar::Header::new_gnu();
    let name = "light-widget-datepicker-0.19.0/Cargo.toml";
    header.set_path(name).expect("set path");
    header.set_size(cargo_toml.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    builder
        .append(&header, cargo_toml.as_bytes())
        .expect("append");
    let tar_bytes = builder.into_inner().expect("into inner");

    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&tar_bytes).expect("write all");
    encoder.finish().expect("finish")
}

/// Создаёт RegistryConfig с указанными URL.
fn test_config(base: &str, download: &str) -> RegistryConfig {
    RegistryConfig {
        url: base.to_string(),
        download_url: download.to_string(),
        docs_url: "https://docs.rs".to_string(),
        source: light_fw_cli_core::registry_config::ConfigSource::Builtin,
    }
}

#[tokio::test]
async fn search_ranking_official_first() {
    let server = MockServer::start().await;

    // Search API: возвращаем 3 результата.
    let search_body = serde_json::json!({
        "crates": [
            {
                "name": "light-widget-alpha",
                "max_version": "1.0.0",
                "newest_version": "1.0.0",
                "max_stable_version": "1.0.0",
                "description": "Alpha widget for Light forms",
                "downloads": 5000
            },
            {
                "name": "light-widget-beta",
                "max_version": "1.0.0",
                "newest_version": "1.0.0",
                "max_stable_version": "1.0.0",
                "description": "A component called light-widget-beta",
                "downloads": 100
            },
            {
                "name": "light-widget-gamma",
                "max_version": "1.0.0",
                "newest_version": "1.0.0",
                "max_stable_version": "1.0.0",
                "description": "Gamma widget, no name in description",
                "downloads": 9999
            }
        ]
    })
    .to_string();

    Mock::given(method("GET"))
        .and(path_regex(r"/api/v1/crates\?.*"))
        .respond_with(ResponseTemplate::new(200).set_body_string(search_body))
        .mount(&server)
        .await;

    // Owners API: alpha → gifoff (official), beta/gamma → none.
    Mock::given(method("GET"))
        .and(path_regex(r"/api/v1/crates/light-widget-alpha/owners.*"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(r#"{"users":[{"login":"gifoff"}]}"#),
        )
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path_regex(r"/api/v1/crates/light-widget-beta/owners.*"))
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"users":[]}"#))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path_regex(r"/api/v1/crates/light-widget-gamma/owners.*"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    let config = test_config(&server.uri(), &server.uri());
    let client = RegistryClient::new(config);

    let raw = client.search("widget", 30).expect("search");
    let filtered = filter_by_prefix(&raw);
    assert_eq!(filtered.len(), 3, "all three should pass prefix filter");

    // Build SearchResult items with official flags from owners check.
    let mut results: Vec<SearchResult> = Vec::new();
    for item in &filtered {
        let owners = client.owners(&item.name).unwrap_or_default();
        let official = owners.iter().any(|o| o == "gifoff");
        results.push(SearchResult {
            name: item.name.clone(),
            version: item.version.clone(),
            description: truncate_description(item.description.as_deref()),
            official,
            downloads: item.downloads,
            link: item.url.clone(),
            already_installed: false,
        });
    }

    let ranked = rank(&results, "widget");
    // 1st: alpha (official — always first)
    assert_eq!(ranked[0].name, "light-widget-alpha");
    assert!(ranked[0].official);
    // 2nd/3rd: beta and gamma both have "widget" in name → tie-breaker
    // falls through to downloads: beta (1000) > gamma (100).
    assert_eq!(ranked[1].name, "light-widget-beta");
    assert_eq!(ranked[2].name, "light-widget-gamma");
}

#[tokio::test]
async fn prefix_filter_excludes_non_light() {
    let server = MockServer::start().await;

    let search_body = serde_json::json!({
        "crates": [
            {"name": "light-fw-datepicker", "max_version": "1.0.0", "newest_version": "1.0.0", "max_stable_version": "1.0.0", "description": "Date picker", "downloads": 100},
            {"name": "my-datepicker", "max_version": "1.0.0", "newest_version": "1.0.0", "max_stable_version": "1.0.0", "description": "Another date picker", "downloads": 500},
            {"name": "light-element-button", "max_version": "1.0.0", "newest_version": "1.0.0", "max_stable_version": "1.0.0", "description": "Button element", "downloads": 200},
            {"name": "serde", "max_version": "1.0.0", "newest_version": "1.0.0", "max_stable_version": "1.0.0", "description": "Serialization", "downloads": 100000}
        ]
    })
    .to_string();

    Mock::given(method("GET"))
        .and(path_regex(r"/api/v1/crates\?.*"))
        .respond_with(ResponseTemplate::new(200).set_body_string(search_body))
        .mount(&server)
        .await;

    let config = test_config(&server.uri(), &server.uri());
    let client = RegistryClient::new(config);

    let raw = client.search("date", 30).expect("search");
    let filtered = filter_by_prefix(&raw);
    assert_eq!(filtered.len(), 2);
    assert_eq!(filtered[0].name, "light-fw-datepicker");
    assert_eq!(filtered[1].name, "light-element-button");
}

#[tokio::test]
async fn download_and_validate_ok_manifest() {
    let server = MockServer::start().await;

    let cargo_toml = r#"
[package]
name = "light-widget-datepicker"
version = "0.19.0"
edition = "2024"

[dependencies]

[package.metadata.light]
kind = "widget"
core = "^0.18"

[package.metadata.light.provides]
elements = ["datepicker"]
attributes = []
directives = []

[package.metadata.light.requires]
components = []
"#;
    let archive = crate_bytes(cargo_toml);

    Mock::given(method("GET"))
        .and(path_regex(r"/light-widget-datepicker/0\.19\.0/download.*"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "application/octet-stream")
                .set_body_bytes(archive),
        )
        .mount(&server)
        .await;

    let config = test_config(&server.uri(), &server.uri());
    let client = RegistryClient::new(config);

    let bytes = client
        .download_crate("light-widget-datepicker", "0.19.0")
        .expect("download");
    let toml_text = light_fw_cli_core::metadata::extract_cargo_toml(&bytes).expect("extract");
    let result =
        light_fw_cli_core::metadata::validate_metadata(&toml_text, "light-widget-datepicker");
    match result {
        light_fw_cli_core::metadata::ValidationResult::Ok(meta) => {
            assert_eq!(meta.crate_name, "light-widget-datepicker");
            assert_eq!(meta.version, "0.19.0");
            assert_eq!(meta.provides_elements, vec!["datepicker".to_string()]);
        }
        other => panic!("expected Ok, got {other:?}"),
    }
}

#[tokio::test]
async fn download_and_validate_no_metadata() {
    let server = MockServer::start().await;

    let cargo_toml = r#"
[package]
name = "light-widget-broken"
version = "0.1.0"
edition = "2024"
"#;
    let archive = crate_bytes(cargo_toml);

    Mock::given(method("GET"))
        .and(path_regex(r"/light-widget-broken/0\.1\.0/download.*"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "application/octet-stream")
                .set_body_bytes(archive),
        )
        .mount(&server)
        .await;

    let config = test_config(&server.uri(), &server.uri());
    let client = RegistryClient::new(config);

    let bytes = client
        .download_crate("light-widget-broken", "0.1.0")
        .expect("download");
    let toml_text = light_fw_cli_core::metadata::extract_cargo_toml(&bytes).expect("extract");
    let result = light_fw_cli_core::metadata::validate_metadata(&toml_text, "light-widget-broken");
    match result {
        light_fw_cli_core::metadata::ValidationResult::Fatal(err) => {
            let msg = light_fw_cli_core::metadata::ValidationError::message(&err);
            assert_eq!(msg, "Нет метаданных [package.metadata.light] в Cargo.toml");
        }
        other => panic!("expected Fatal(NoMetadata), got {other:?}"),
    }
}

#[tokio::test]
async fn download_and_validate_incompatible_core() {
    let server = MockServer::start().await;

    let cargo_toml = r#"
[package]
name = "light-widget-future"
version = "0.1.0"
edition = "2024"

[package.metadata.light]
kind = "widget"
core = "^99"

[package.metadata.light.provides]
elements = ["future"]
attributes = []
directives = []

[package.metadata.light.requires]
components = []
"#;
    let archive = crate_bytes(cargo_toml);

    Mock::given(method("GET"))
        .and(path_regex(r"/light-widget-future/0\.1\.0/download.*"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "application/octet-stream")
                .set_body_bytes(archive),
        )
        .mount(&server)
        .await;

    let config = test_config(&server.uri(), &server.uri());
    let client = RegistryClient::new(config);

    let bytes = client
        .download_crate("light-widget-future", "0.1.0")
        .expect("download");
    let toml_text = light_fw_cli_core::metadata::extract_cargo_toml(&bytes).expect("extract");
    let result = light_fw_cli_core::metadata::validate_metadata(&toml_text, "light-widget-future");
    match result {
        light_fw_cli_core::metadata::ValidationResult::Fatal(err) => {
            let msg = light_fw_cli_core::metadata::ValidationError::message(&err);
            assert!(
                msg.contains("99"),
                "expected incompatible core error, got: {msg}"
            );
        }
        other => panic!("expected Fatal(IncompatibleCore), got {other:?}"),
    }
}
