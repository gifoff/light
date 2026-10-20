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
}
