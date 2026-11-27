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
use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Строит tar.gz архив с `Cargo.toml` внутри.
fn crate_bytes(cargo_toml: &str) -> Vec<u8> {
    use flate2::Compression;
    use flate2::write::GzEncoder;
    use std::io::Write;

    let mut builder = tar::Builder::new(Vec::new());
    let mut header = tar::Header::new_gnu();
    let name = "light-widget-datepicker-0.20.0/Cargo.toml";
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

#[tokio::test(flavor = "multi_thread")]
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
        .and(path("/api/v1/crates"))
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

    // RegistryClient использует reqwest::blocking (создаёт свой runtime).
    // В async-контексте drop этого runtime pan'ит. Выносим все sync-вызовы
    // в spawn_blocking, чтобы они выполнялись на отдельном worker-thread.
    let uri = server.uri();
    let raw = {
        let u = uri.clone();
        tokio::task::spawn_blocking(move || {
            let config = test_config(&u, &u);
            let client = RegistryClient::new(config);
            client.search("widget", 30).expect("search")
        })
        .await
        .expect("spawn_blocking join")
    };
    let filtered = filter_by_prefix(&raw);
    assert_eq!(filtered.len(), 3, "all three should pass prefix filter");

    // Build SearchResult items with official flags from owners check.
    // Owners — тоже blocking, выносим в spawn_blocking.
    let names: Vec<String> = filtered.iter().map(|i| i.name.clone()).collect();
    let owners_map = {
        let u = uri.clone();
        let nms = names;
        tokio::task::spawn_blocking(move || {
            let config = test_config(&u, &u);
            let client = RegistryClient::new(config);
            nms.into_iter()
                .map(|n| {
                    let owners = client.owners(&n).unwrap_or_default();
                    (n, owners)
                })
                .collect::<Vec<_>>()
        })
        .await
        .expect("spawn_blocking join")
    };

    let mut results: Vec<SearchResult> = Vec::new();
    for item in &filtered {
        let owners = owners_map
            .iter()
            .find(|(n, _)| n == &item.name)
            .map(|(_, o)| o.clone())
            .unwrap_or_default();
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
    // 2nd/3rd: gamma (9999) > beta (100) by downloads.
    assert_eq!(ranked[1].name, "light-widget-gamma");
    assert_eq!(ranked[2].name, "light-widget-beta");
}

#[tokio::test(flavor = "multi_thread")]
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
        .and(path("/api/v1/crates"))
        .respond_with(ResponseTemplate::new(200).set_body_string(search_body))
        .mount(&server)
        .await;

    let uri = server.uri();
    let raw = tokio::task::spawn_blocking(move || {
        let config = test_config(&uri, &uri);
        let client = RegistryClient::new(config);
        client.search("date", 30).expect("search")
    })
    .await
    .expect("spawn_blocking join");
    let filtered = filter_by_prefix(&raw);
    assert_eq!(filtered.len(), 2);
    assert_eq!(filtered[0].name, "light-fw-datepicker");
    assert_eq!(filtered[1].name, "light-element-button");
}

#[tokio::test(flavor = "multi_thread")]
async fn download_and_validate_ok_manifest() {
    let server = MockServer::start().await;

    // Фикстура core-версии выводится из версии крейта (env! CARGO_PKG_VERSION):
    // pre-release (0.21.0-alpha.1) → ^0.21-0 (карет на уровне major.minor),
    // stable (0.21.0) → ^0.21.
    // Семан: ^0.21-0 = >=0.21-0 < 0.22.0, и все pre-release'ы 0.21.x
    // (alpha, beta, rc…) попадают в диапазон. См. semver 2.0 pre-release rule.
    let crate_version = env!("CARGO_PKG_VERSION");
    let core_req = if crate_version.contains('-') {
        // Pre-release: базовая часть до первого `-` + `-0`.
        let base = crate_version.split('-').next().unwrap_or(crate_version);
        format!("{base}-0")
    } else {
        // Stable: major.minor (caret).
        let parts: Vec<&str> = crate_version.split('.').collect();
        if parts.len() >= 2 {
            format!("^{}.{}", parts[0], parts[1])
        } else {
            crate_version.to_string()
        }
    };

    let cargo_toml = format!(
        r#"
[package]
name = "light-widget-datepicker"
version = "0.20.0"
edition = "2024"

[dependencies]

[package.metadata.light]
kind = "widget"
display-name = "Datepicker"
core = "{core_req}"

[package.metadata.light.provides]
elements = ["datepicker"]
attributes = []
directives = []

[package.metadata.light.requires]
components = []
"#
    );
    let archive = crate_bytes(&cargo_toml);

    Mock::given(method("GET"))
        .and(path_regex(r"/light-widget-datepicker/0\.20\.0/download.*"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "application/octet-stream")
                .set_body_bytes(archive),
        )
        .mount(&server)
        .await;

    let uri = server.uri();
    let result = tokio::task::spawn_blocking(move || {
        let config = test_config(&uri, &uri);
        let client = RegistryClient::new(config);
        let bytes = client
            .download_crate("light-widget-datepicker", "0.20.0")
            .expect("download");
        let toml_text = light_fw_cli_core::metadata::extract_cargo_toml(&bytes).expect("extract");
        light_fw_cli_core::metadata::validate_metadata(&toml_text, "light-widget-datepicker")
    })
    .await
    .expect("spawn_blocking join");
    match result {
        light_fw_cli_core::metadata::ValidationResult::Ok(meta) => {
            assert_eq!(meta.crate_name, "light-widget-datepicker");
            assert_eq!(meta.version, "0.20.0");
            assert_eq!(meta.provides_elements, vec!["datepicker".to_string()]);
        }
        other => panic!("expected Ok, got {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
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

    let uri = server.uri();
    let result = tokio::task::spawn_blocking(move || {
        let config = test_config(&uri, &uri);
        let client = RegistryClient::new(config);
        let bytes = client
            .download_crate("light-widget-broken", "0.1.0")
            .expect("download");
        let toml_text = light_fw_cli_core::metadata::extract_cargo_toml(&bytes).expect("extract");
        light_fw_cli_core::metadata::validate_metadata(&toml_text, "light-widget-broken")
    })
    .await
    .expect("spawn_blocking join");
    match result {
        light_fw_cli_core::metadata::ValidationResult::Fatal(err) => {
            let msg = light_fw_cli_core::metadata::ValidationError::message(&err);
            assert!(msg.contains("[package.metadata.light]"), "получено: {msg}");
            assert!(msg.contains("не компонент Light"), "получено: {msg}");
        }
        other => panic!("expected Fatal(NoMetadata), got {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn download_and_validate_incompatible_core() {
    let server = MockServer::start().await;

    let cargo_toml = r#"
[package]
name = "light-widget-future"
version = "0.1.0"
edition = "2024"

[package.metadata.light]
kind = "widget"
display-name = "Future"
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

    let uri = server.uri();
    let result = tokio::task::spawn_blocking(move || {
        let config = test_config(&uri, &uri);
        let client = RegistryClient::new(config);
        let bytes = client
            .download_crate("light-widget-future", "0.1.0")
            .expect("download");
        let toml_text = light_fw_cli_core::metadata::extract_cargo_toml(&bytes).expect("extract");
        light_fw_cli_core::metadata::validate_metadata(&toml_text, "light-widget-future")
    })
    .await
    .expect("spawn_blocking join");
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
