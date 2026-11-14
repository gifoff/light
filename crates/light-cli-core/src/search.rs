//! Команда `light search`: поиск компонентов на crates.io.
//!
//! Сетевой запрос спрятан за [`RemoteRegistry`], поэтому фильтрация и вывод
//! таблицы тестируются без сети.

use crate::error::Result;
use crate::fetch::{CrateSearchResult, RemoteRegistry};

/// Сколько результатов запрашивать у crates.io.
pub const SEARCH_PER_PAGE: usize = 10;

const NAME_WIDTH: usize = 30;
const VERSION_WIDTH: usize = 10;
const DESCRIPTION_WIDTH: usize = 60;

/// Ищет компоненты Light по запросу.
///
/// Показываются только крейты `light-fw-*` или крейты, в описании которых
/// упоминается `light`.
pub fn search(remote: &dyn RemoteRegistry, query: &str) -> Result<Vec<CrateSearchResult>> {
    Ok(filter_components(remote.search(query, SEARCH_PER_PAGE)?))
}

/// Оставляет только релевантные компоненты Light.
pub fn filter_components(results: Vec<CrateSearchResult>) -> Vec<CrateSearchResult> {
    results.into_iter().filter(is_component_result).collect()
}

fn is_component_result(result: &CrateSearchResult) -> bool {
    result.name.to_ascii_lowercase().starts_with("light-fw-")
        || result
            .description
            .as_deref()
            .is_some_and(|description| description.to_lowercase().contains("light"))
}

/// Форматирует результаты в текстовую таблицу.
pub fn render_table(results: &[CrateSearchResult]) -> String {
    let mut out = format!(
        "{:<NAME_WIDTH$} {:<VERSION_WIDTH$} {:<DESCRIPTION_WIDTH$} {}\n",
        "NAME", "VERSION", "DESCRIPTION", "URL"
    );
    for result in results {
        out.push_str(&format!(
            "{:<NAME_WIDTH$} {:<VERSION_WIDTH$} {:<DESCRIPTION_WIDTH$} {}\n",
            result.name,
            result.version,
            brief(result.description.as_deref().unwrap_or("")),
            result.url
        ));
    }
    out
}

fn brief(text: &str) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.chars().count() <= DESCRIPTION_WIDTH {
        return text;
    }
    let mut brief: String = text.chars().take(DESCRIPTION_WIDTH - 1).collect();
    brief.push('…');
    brief
}

// ─── Этап 0: расширенный поиск с реестром ────────────────────────────────

/// Опции расширенного поиска (Этап 0).
#[derive(Debug, Clone)]
pub struct SearchOptions {
    /// Запрос поиска.
    pub query: String,
    /// Максимум результатов (по умолчанию 30).
    pub limit: usize,
    /// Ненормативный флаг: автоматически выбрать первый официальный.
    pub yes: bool,
    /// Вывод в JSON-формате (для скриптов).
    pub json: bool,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            query: String::new(),
            limit: 30,
            yes: false,
            json: false,
        }
    }
}

/// Результат расширенного поиска (Этап 0).
#[derive(Debug, Clone, serde::Serialize)]
pub struct SearchResult {
    /// Имя крейта.
    pub name: String,
    /// Версия (максимальная стабильная).
    pub version: String,
    /// Описание (обрезано до 200 символов + '…').
    pub description: String,
    /// Официальный компонент (owner содержит "gifoff").
    pub official: bool,
    /// Количество загрузок.
    pub downloads: u64,
    /// Ссылка на страницу крейта.
    pub link: String,
    /// Уже установлен в проекте (`[dependencies]` в `Cargo.toml`).
    pub already_installed: bool,
}

/// Префиксы имён Light-компонентов для фильтрации.
pub const LIGHT_PREFIXES: &[&str] = &[
    "light-fw-",
    "light-widget-",
    "light-element-",
    "light-behavior-",
    "light-integration-",
    "light-subsystem-",
];

/// Оставляет только крейты с Light-префиксами (по имени).
pub fn filter_by_prefix(results: &[CrateSearchResult]) -> Vec<CrateSearchResult> {
    results
        .iter()
        .filter(|r| {
            LIGHT_PREFIXES
                .iter()
                .any(|prefix| r.name.starts_with(prefix))
        })
        .cloned()
        .collect()
}

/// Обрезает описание до 200 символов (Unicode-safe) с добавлением '…'.
pub fn truncate_description(desc: Option<&str>) -> String {
    match desc {
        None => String::new(),
        Some(text) => {
            let normalized: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
            if normalized.chars().count() <= 200 {
                return normalized;
            }
            let mut truncated: String = normalized.chars().take(200).collect();
            truncated.push('…');
            truncated
        }
    }
}

/// Ранжирует результаты: официальный > совпадение имени с запросом > загрузки.
pub fn rank(results: &[SearchResult], query: &str) -> Vec<SearchResult> {
    let query_lower = query.to_lowercase();
    let mut sorted: Vec<&SearchResult> = results.iter().collect();
    sorted.sort_by(|a, b| {
        // 1. Официальные — наверх.
        match (a.official, b.official) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => {
                // 2. Имя содержит запрос (тайбрейкер).
                let a_name_match = a.name.to_lowercase().contains(&query_lower);
                let b_name_match = b.name.to_lowercase().contains(&query_lower);
                match (a_name_match, b_name_match) {
                    (true, false) => std::cmp::Ordering::Less,
                    (false, true) => std::cmp::Ordering::Greater,
                    _ => {
                        // 3. По количеству загрузок (больше — выше).
                        b.downloads.cmp(&a.downloads)
                    }
                }
            }
        }
    });
    sorted.into_iter().cloned().collect()
}

/// Проверка, установлен ли компонент уже в проект.
///
/// Ищет имя в `[dependencies]` корневого `Cargo.toml`.
pub fn is_already_installed(name: &str, project_root: &std::path::Path) -> bool {
    let cargo_toml = project_root.join("Cargo.toml");
    let Ok(content) = std::fs::read_to_string(&cargo_toml) else {
        return false;
    };
    // Простой разбор: ищем секцию [dependencies] и строки вида `name =`.
    let lines: Vec<&str> = content.lines().collect();
    let mut in_deps = false;
    for line in &lines {
        let trimmed = line.trim();
        if trimmed == "[dependencies]" {
            in_deps = true;
            continue;
        }
        if trimmed.starts_with('[') && trimmed != "[dependencies]" {
            in_deps = false;
            continue;
        }
        if in_deps {
            let name_part = trimmed.split(['=', ' ']).next().unwrap_or("").trim();
            if name_part == name {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockRemote {
        results: Vec<CrateSearchResult>,
    }

    impl RemoteRegistry for MockRemote {
        fn search(&self, _query: &str, _per_page: usize) -> Result<Vec<CrateSearchResult>> {
            Ok(self.results.clone())
        }
    }

    fn result(name: &str, description: Option<&str>) -> CrateSearchResult {
        CrateSearchResult {
            name: name.to_string(),
            version: "0.3.0".to_string(),
            description: description.map(str::to_string),
            url: format!("https://crates.io/crates/{name}"),
            downloads: 0,
            owners: Vec::new(),
        }
    }

    #[test]
    fn filters_to_light_components() {
        let remote = MockRemote {
            results: vec![
                result("light-fw-datepicker", Some("Date picker")),
                result("time-utils", Some("A Light component for dates")),
                result("serde", Some("Serialization framework")),
            ],
        };
        let found = search(&remote, "date").expect("search");
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].name, "light-fw-datepicker");
        assert_eq!(found[1].name, "time-utils");
    }

    #[test]
    fn table_has_columns_and_links() {
        let results = vec![result("light-fw-datepicker", Some("Date picker for Light"))];
        let table = render_table(&results);
        assert!(table.contains("NAME"), "{table}");
        assert!(table.contains("VERSION"), "{table}");
        assert!(table.contains("DESCRIPTION"), "{table}");
        assert!(table.contains("URL"), "{table}");
        assert!(table.contains("light-fw-datepicker"), "{table}");
        assert!(
            table.contains("https://crates.io/crates/light-fw-datepicker"),
            "{table}"
        );
    }

    #[test]
    fn table_truncates_long_description() {
        let long = "x".repeat(DESCRIPTION_WIDTH + 10);
        let results = vec![result("light-fw-datepicker", Some(&long))];
        let table = render_table(&results);
        assert!(table.contains('…'), "{table}");
    }

    // ─── Этап 0: тесты новых функций ──────────────────────────────────────

    #[test]
    fn rank_officials_first() {
        let results = vec![
            SearchResult {
                name: "light-widget-b".to_string(),
                version: "1.0.0".into(),
                description: "Widget B".into(),
                official: false,
                downloads: 10000,
                link: "https://crates.io/crates/light-widget-b".into(),
                already_installed: false,
            },
            SearchResult {
                name: "light-widget-a".to_string(),
                version: "1.0.0".into(),
                description: "Widget A by gifoff".into(),
                official: true,
                downloads: 100,
                link: "https://crates.io/crates/light-widget-a".into(),
                already_installed: false,
            },
        ];
        let ranked = rank(&results, "widget");
        assert_eq!(ranked[0].name, "light-widget-a");
        assert_eq!(ranked[1].name, "light-widget-b");
    }

    #[test]
    fn rank_both_match_falls_to_downloads() {
        let results = vec![
            SearchResult {
                name: "light-element-x".to_string(),
                version: "1.0.0".into(),
                description: "A component named light-element-x for forms".into(),
                official: false,
                downloads: 50,
                link: String::new(),
                already_installed: false,
            },
            SearchResult {
                name: "light-element-y".to_string(),
                version: "1.0.0".into(),
                description: "Some other element with no name match".into(),
                official: false,
                downloads: 9999,
                link: String::new(),
                already_installed: false,
            },
        ];
        let ranked = rank(&results, "element");
        // Both names contain "element" → fall through to downloads.
        assert_eq!(ranked[0].name, "light-element-y");
    }

    #[test]
    fn rank_query_in_name_beats_downloads() {
        let results = vec![
            SearchResult {
                name: "light-widget-x".to_string(),
                version: "1.0.0".into(),
                description: "unrelated".into(),
                official: false,
                downloads: 9999,
                link: String::new(),
                already_installed: false,
            },
            SearchResult {
                name: "light-widget-datepicker".to_string(),
                version: "1.0.0".into(),
                description: "unrelated".into(),
                official: false,
                downloads: 100,
                link: String::new(),
                already_installed: false,
            },
        ];
        let ranked = rank(&results, "datepicker");
        // datepicker in name wins despite fewer downloads.
        assert_eq!(ranked[0].name, "light-widget-datepicker");
    }

    #[test]
    fn filter_by_prefix_keeps_only_light() {
        let results = vec![
            result("light-fw-datepicker", None),
            result("light-widget-button", None),
            result("my-datepicker", None),
            result("serde", None),
        ];
        let filtered = filter_by_prefix(&results);
        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0].name, "light-fw-datepicker");
        assert_eq!(filtered[1].name, "light-widget-button");
    }

    #[test]
    fn filter_by_prefix_negative_my_datepicker() {
        let results = vec![result("my-datepicker", None)];
        let filtered = filter_by_prefix(&results);
        assert!(filtered.is_empty());
    }

    #[test]
    fn truncate_150_chars_untouched() {
        let text = "a".repeat(150);
        let result = truncate_description(Some(&text));
        assert_eq!(result, text);
        assert!(!result.ends_with('…'));
    }

    #[test]
    fn truncate_300_chars_to_200_plus_ellipsis() {
        let text = "b".repeat(300);
        let result = truncate_description(Some(&text));
        assert_eq!(result.chars().count(), 201); // 200 + '…'
        assert!(result.ends_with('…'));
    }

    #[test]
    fn truncate_none_returns_empty() {
        let result = truncate_description(None);
        assert_eq!(result, "");
    }

    #[test]
    fn truncate_cyrillic_char_boundary() {
        // 200 Cyrillic chars + 50 more (250 total)
        let text = "ж".repeat(250);
        let result = truncate_description(Some(&text));
        assert_eq!(result.chars().count(), 201);
        assert!(result.ends_with('…'));
        // Verify no broken UTF-8
        assert!(std::str::from_utf8(result.as_bytes()).is_ok());
    }
}
