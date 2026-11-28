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

/// Точное имя ядра Light, которое не начинается ни с одного префикса из
/// [`LIGHT_PREFIXES`], но должно проходить фильтр: `light` (фреймворк и
/// сама утилита).
///
/// Расширение до «всё, что начинается с `light`» намеренно НЕ сделано:
/// в выдаче crates.io есть чужие крейты (`light-account`, `light-sdk`,
/// `light-macros`, ...) — они обязаны оставаться отсечёнными.
const LIGHT_CORE_NAME: &str = "light";

/// Имя является Light-крейтом: ровно `light` (ядро) либо один из
/// префиксов [`LIGHT_PREFIXES`] (`light-fw-*` и остальные семейства).
///
/// Сокращение `starts_with("light")` оптимизирует проверку; семантику задаёт
/// точное имя + перечисленные префиксы.
pub fn is_light_crate_name(name: &str) -> bool {
    name == LIGHT_CORE_NAME || LIGHT_PREFIXES.iter().any(|p| name.starts_with(p))
}

/// Оставляет только крейты Light (по имени).
///
/// `light search light` должен находить сам CLI `light`; чужие крейты
/// вроде `light-account` отсечены тем, что не равны `light` и не несут
/// `light-fw-`/`light-widget-`/... префиксов.
pub fn filter_by_prefix(results: &[CrateSearchResult]) -> Vec<CrateSearchResult> {
    results
        .iter()
        .filter(|r| is_light_crate_name(&r.name))
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
        crate_result(name, "0.3.0", description)
    }

    /// [`CrateSearchResult`] с произвольной версией (для ТЗ-тестов).
    fn crate_result(name: &str, version: &str, description: Option<&str>) -> CrateSearchResult {
        CrateSearchResult {
            name: name.to_string(),
            version: version.to_string(),
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

    // ─── FIX8: `light search light` должен находить сам CLI `light` ──────

    /// ТЗ: запрос `light` через mock-реестр содержит `light 0.21.0-alpha.1`.
    ///
    /// Набор результатов имитирует живой ответ crates.io на `q=light`
    /// (проба `fix8-t0-prefilter.txt`): `light`, `seiza-stacking`,
    /// `light-account`, `native-theme-gpui`, `light-sdk`, ...
    #[test]
    fn search_light_query_finds_core_cli_crate() {
        let remote = MockRemote {
            results: vec![
                crate_result("light", "0.21.0-alpha.1", Some("Утилита light")),
                crate_result("seiza-stacking", "0.5.0", Some("Stacking library")),
                crate_result("light-account", "1.0.0", None),
                crate_result("light-sdk", "2.0.0", None),
                crate_result("serde", "1.0.0", Some("Serialization framework")),
            ],
        };
        let found = filter_by_prefix(&remote.search("light", 30).expect("search"));
        let names: Vec<&str> = found.iter().map(|r| r.name.as_str()).collect();
        assert!(
            names.contains(&"light"),
            "`light` (CLI) должен пройти фильтр: {names:?}"
        );
        // Третьи стороны отсечены фильтром по имени (регрессия до фикса:
        // `light` сам не проходил, и всё было «Ничего не найдено»).
        assert!(!names.contains(&"light-account"));
        assert!(!names.contains(&"light-sdk"));
        assert!(!names.contains(&"seiza-stacking"));
        assert!(!names.contains(&"serde"));
        // Проверка версии из ТЗ.
        let core = found
            .iter()
            .find(|r| r.name == "light")
            .expect("found above");
        assert_eq!(core.version, "0.21.0-alpha.1");
    }

    /// Регрессия FIX8 (до правки): `light search light` возвращал
    /// «Ничего не найдено», т.к. ни один результат живого поиска не
    /// начинался с `LIGHT_PREFIXES`.
    #[test]
    fn live_query_light_not_all_filtered_out() {
        // Точный состав ответа crates.io `q=light&per_page=30` (проба
        // fix8-t0-prefilter.txt); до фикса фильтр отбрасывал ВСЕ 30.
        let results: Vec<CrateSearchResult> = [
            "light",
            "seiza-stacking",
            "light-account",
            "native-theme-gpui",
            "light-sdk",
            "tendermint-light-client",
            "dark-light",
            "terminal-light",
            "bevy_voxel_world",
            "tui-nuances",
            "colorimetry",
            "bevy_light_2d",
            "flight-lighting",
            "oxideav-gltf",
            "light-client",
            "seiza-calibration",
            "light-curve-feature",
            "light-batched-merkle-tree",
            "arborium-theme",
            "bevy_firefly",
            "egui_bench",
            "rs-matter",
            "light-macros",
            "light-poseidon-nostd",
            "giallo",
            "tendermint-light-client-detector",
            "light-merkle-tree-metadata",
            "light-curve-dmdt",
            "light-event",
            "hns-light-p2p",
        ]
        .iter()
        .map(|name| result(name, None))
        .collect();
        let filtered = filter_by_prefix(&results);
        assert_eq!(filtered.len(), 1, "только сам `light` должен пройти");
        assert_eq!(filtered[0].name, "light");
    }

    /// ТЗ: запрос `light-fw` содержит `light-fw-element-form` и
    /// `light-fw-behavior-validation`.
    #[test]
    fn search_light_fw_query_finds_published_components() {
        let remote = MockRemote {
            results: vec![
                crate_result("light-fw-element-form", "0.17.0", Some("Элементы форм")),
                crate_result("light-fw-behavior-validation", "0.17.0", Some("Валидация")),
                crate_result("light", "0.21.0-alpha.1", Some("Утилита light")),
                crate_result("light-account", "1.0.0", None),
            ],
        };
        let found = filter_by_prefix(&remote.search("light-fw", 30).expect("search"));
        let names: Vec<&str> = found.iter().map(|r| r.name.as_str()).collect();
        // Регресс-условие приёмки: чужие `light-*` отсечены.
        assert!(
            !names.contains(&"light-account"),
            "чужой light-* должен быть отсечён: {names:?}"
        );
        // Ядро `light` (опубликовано на crates.io) тоже проходит.
        assert!(names.contains(&"light"));
        // Обязательные компоненты ТЗ:
        assert!(
            names.contains(&"light-fw-element-form"),
            "обязательный компонент отсутствует: {names:?}"
        );
        assert!(
            names.contains(&"light-fw-behavior-validation"),
            "обязательный компонент отсутствует: {names:?}"
        );
    }

    /// Ровно `light` (ядро) и префиксы из [`LIGHT_PREFIXES`] (`light-fw-*`,
    /// `light-widget-*`, ...) проходят; мета-крейт `light-fw` (без дефиса)
    /// и чужие `light-*` — нет (условие приёмки №1).
    #[test]
    fn is_light_crate_name_exact_and_prefix() {
        assert!(
            is_light_crate_name("light"),
            "ядро `light` должно проходить"
        );
        assert!(is_light_crate_name("light-fw-ui"));
        assert!(is_light_crate_name("light-widget-datepicker"));
        assert!(is_light_crate_name("light-element-form"));
        assert!(is_light_crate_name("light-fw-element-form"));
        assert!(is_light_crate_name("light-behavior-validation"));
        // Не Light:
        assert!(
            !is_light_crate_name("light-fw"),
            "мета-крейт без префикса — не компонент"
        );
        assert!(
            !is_light_crate_name("light-account"),
            "чужой light-* отсечён"
        );
        assert!(!is_light_crate_name("light-sdk"));
        assert!(!is_light_crate_name("light-macros"));
        assert!(!is_light_crate_name("flight-lighting"));
        assert!(!is_light_crate_name("my-datepicker"));
        assert!(!is_light_crate_name("serde"));
    }

    /// `light-fw-ui` (не опубликован) при запросе `light-fw` не появляется —
    /// в реестре такого крейта просто нет; mock имитирует живой ответ.
    #[test]
    fn search_light_fw_ui_not_in_live_results() {
        let remote = MockRemote {
            results: vec![
                crate_result("light-fw-html", "0.20.2", Some("HTML")),
                crate_result("light-fw-macros", "0.20.2", Some("Макросы")),
                crate_result("light-fw-core", "0.20.2", Some("Ядро")),
                crate_result("light-fw-cli-core", "0.20.2", Some("CLI")),
                crate_result("light-fw-element-form", "0.20.2", Some("Элементы форм")),
                crate_result("light-fw-behavior-validation", "0.20.2", Some("Валидация")),
                crate_result("light", "0.21.0-alpha.1", Some("Утилита")),
                crate_result("light-fw-devtools-web", "0.20.2", Some("DevTools")),
            ],
        };
        let found = filter_by_prefix(&remote.search("light-fw", 30).expect("search"));
        let names: Vec<&str> = found.iter().map(|r| r.name.as_str()).collect();
        assert!(
            !names.contains(&"light-fw-ui"),
            "`light-fw-ui` не опубликован на crates.io: {names:?}"
        );
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
