use super::types::CrateSearchResult;
use crate::error::Result;
/// Быстрая проверка доступности crates.io (HEAD-запрос с таймаутом 5 с).
pub fn ping_crates_io() -> bool {
    use std::time::Duration;
    let client = match reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
    {
        Ok(c) => c,
        Err(_) => return false,
    };
    client
        .get("https://crates.io/api/v1/crates/light")
        .header("User-Agent", "light-fw-cli (diagnostics)")
        .send()
        .is_ok()
}

/// Разбирает ответ `GET /api/v1/crates?q=...`.
#[cfg(feature = "network")]
fn parse_search_response(
    body: &str,
    config: &crate::registry_config::RegistryConfig,
) -> Result<Vec<CrateSearchResult>> {
    #[derive(serde::Deserialize)]
    struct Response {
        #[serde(default)]
        crates: Vec<Entry>,
    }
    #[derive(serde::Deserialize)]
    struct Entry {
        name: String,
        #[serde(default)]
        max_stable_version: Option<String>,
        #[serde(default)]
        max_version: Option<String>,
        #[serde(default)]
        description: Option<String>,
        #[serde(default)]
        downloads: u64,
    }

    let response: Response = serde_json::from_str(body)?;
    Ok(response
        .crates
        .into_iter()
        .map(|entry| CrateSearchResult {
            version: entry
                .max_stable_version
                .or(entry.max_version)
                .unwrap_or_default(),
            url: format!("{}/crates/{}", config.url.trim_end_matches('/'), entry.name),
            name: entry.name,
            description: entry.description,
            downloads: entry.downloads,
            owners: Vec::new(),
        })
        .collect())
}

/// Ищет крейты на crates.io через API.
#[cfg(feature = "network")]
pub fn fetch_crates_io_search(
    query: &str,
    per_page: usize,
    config: &crate::registry_config::RegistryConfig,
) -> Result<Vec<CrateSearchResult>> {
    use std::time::Duration;

    let per_page = per_page.to_string();
    let base = config.url.trim_end_matches('/');
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()?;
    let response = client
        .get(format!("{base}/api/v1/crates"))
        .query(&[
            ("q", query),
            ("per_page", per_page.as_str()),
            ("sort", "relevance"),
        ])
        .header("User-Agent", "light-fw-cli (component search)")
        .send()?
        .error_for_status()?;
    let body = response.text()?;
    parse_search_response(&body, config)
}

/// Запрашивает последнюю версию крейта на crates.io.
///
/// Возвращает `Ok(None)`, если крейт не найден (HTTP 404).
#[cfg(feature = "network")]
pub fn fetch_crates_io(
    name: &str,
    config: &crate::registry_config::RegistryConfig,
) -> Result<Option<semver::Version>> {
    use std::time::Duration;

    #[derive(serde::Deserialize)]
    struct CrateResponse {
        #[serde(rename = "crate")]
        krate: CrateInfo,
    }
    #[derive(serde::Deserialize)]
    struct CrateInfo {
        max_stable_version: Option<String>,
        max_version: String,
    }

    let base = config.url.trim_end_matches('/');
    let url = format!("{base}/api/v1/crates/{name}");
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()?;
    let response = client
        .get(&url)
        .header("User-Agent", "light-fw-cli (component manager)")
        .send()?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let response = response.error_for_status()?;
    let parsed: CrateResponse = response.json()?;
    let raw = parsed
        .krate
        .max_stable_version
        .unwrap_or(parsed.krate.max_version);
    let version = semver::Version::parse(&raw)?;
    Ok(Some(version))
}

/// Запрашивает владельцев крейта: `GET /api/v1/crates/<name>/owners`.
///
/// Таймаут 2 с. При любой ошибке (сеть, 404, rate-limit) возвращает
/// пустой вектор — вызывающий интерпретирует как «неизвестно» и не
/// ставит бейдж official.
#[cfg(feature = "network")]
pub fn fetch_owners(
    name: &str,
    config: &crate::registry_config::RegistryConfig,
) -> Result<Vec<String>> {
    use std::time::Duration;

    #[derive(serde::Deserialize)]
    struct OwnersResponse {
        #[serde(default)]
        users: Vec<OwnerEntry>,
    }
    #[derive(serde::Deserialize)]
    struct OwnerEntry {
        login: String,
    }

    let base = config.url.trim_end_matches('/');
    let url = format!("{base}/api/v1/crates/{name}/owners");
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()?;
    match client
        .get(&url)
        .header("User-Agent", "light-fw-cli (owners check)")
        .send()?
    {
        response if response.status() == reqwest::StatusCode::NOT_FOUND => Ok(Vec::new()),
        response => {
            let response = response.error_for_status()?;
            let body = response.text()?;
            let parsed: OwnersResponse = serde_json::from_str(&body)?;
            Ok(parsed.users.into_iter().map(|u| u.login).collect())
        }
    }
}

/// Скачивает `.crate`-архив: `GET <download_url>/<name>/<version>/download`.
#[cfg(feature = "network")]
pub fn fetch_crate_archive(
    name: &str,
    version: &str,
    config: &crate::registry_config::RegistryConfig,
) -> Result<Vec<u8>> {
    use std::time::Duration;

    let base = config.download_url.trim_end_matches('/');
    let url = format!("{base}/{name}/{version}/download");
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;
    let response = client
        .get(&url)
        .header("User-Agent", "light-fw-cli (crate download)")
        .send()?
        .error_for_status()?;
    Ok(response.bytes()?.to_vec())
}
