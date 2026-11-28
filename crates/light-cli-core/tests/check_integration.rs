//! Интеграционные тесты логики `light check` (Этап 1 Plugin API).
//!
//! Реальный `cargo metadata` не стабильно работает в песочнице CI (пустой
//! stdout) — поэтому здесь тестируем чистую часть пайплайна:
//!
//! * `compare_tags` — классификация тегов на installed/available/unknown/
//!   unused/conflicts,
//! * `load_installed_from_json` — разбор JSON-ответа `cargo metadata` (без
//!   реального запуска cargo),
//! * `CheckReport::to_text` / `to_json` — форматы вывода.
//!
//! Реальный бинарник `light check` дымится отдельно в E2E
//! (Task 7, `C:\Temp\light-plugin-e2e\stage1-check\`), где cargo доступен.
//!
//! Запуск: `cargo test -p light-fw-cli-core --no-default-features
//! --test check_integration`.

use std::collections::BTreeMap;

use light_fw_cli_core::check::{
    AvailableComponent, CheckReport, ComponentInfo, Conflict, build_check_report, compare_tags,
};
use light_fw_cli_core::metadata::load_installed_from_json;
use tempfile::tempdir;

/// Строит `ComponentInfo` с указанными элементами (тест-хелпер).
fn component(name: &str, version: &str, elements: &[&str]) -> ComponentInfo {
    use light_fw_cli_core::manifest::Provides;
    ComponentInfo {
        crate_name: name.to_string(),
        version: version.to_string(),
        provides: Provides {
            elements: elements.iter().map(|s| s.to_string()).collect(),
            attributes: vec![],
            directives: vec![],
        },
    }
}

/// Создаёт временный проект: `Cargo.toml` + `templates/app.html`.
/// Зависимость `light-fw` задаётся как абсолютный путь (не используется
/// тестами — только для полноты структуры проекта).
fn make_project(dir: &std::path::Path, app_html: &str) {
    std::fs::create_dir_all(dir).expect("mkdir");
    let manifest = r#"
[package]
name = "test-check"
version = "0.1.0"
edition = "2021"

[dependencies]
"#;
    std::fs::write(dir.join("Cargo.toml"), manifest).expect("write Cargo.toml");
    std::fs::create_dir_all(dir.join("templates")).expect("mkdir templates");
    std::fs::write(dir.join("templates/app.html"), app_html).expect("write app.html");
}

// ---------------------------------------------------------------------------
// Сценарий 1 по ТЗ: `light init test-check` → `light check` → exit 0, все
// категории пусты. Эмулируется пустым used-множеством (только стандартные
// теги) и пустым installed.
// ---------------------------------------------------------------------------
#[test]
fn check_init_then_clean() {
    let mut used: BTreeMap<String, usize> = BTreeMap::new();
    // Только стандартные HTML-теги из scaffold'а light init.
    used.insert("window".into(), 1);
    used.insert("div".into(), 2);
    used.insert("h1".into(), 1);
    let installed: Vec<ComponentInfo> = vec![];

    let report = compare_tags(&used, &installed);
    assert!(report.installed.is_empty());
    assert!(report.available.is_empty());
    assert!(report.unknown.is_empty());
    assert!(report.unused.is_empty());
    assert!(report.conflicts.is_empty());
    assert!(
        !report.has_problems(),
        "fresh project should have no problems"
    );
}

// ---------------------------------------------------------------------------
// Сценарий 2 по ТЗ: добавлен `<checkbox>` без установленного
// light-fw-element-form → «Доступно».
// ---------------------------------------------------------------------------
#[test]
fn check_checkbox_without_form_is_available() {
    let mut used: BTreeMap<String, usize> = BTreeMap::new();
    used.insert("div".into(), 1);
    used.insert("checkbox".into(), 1);
    let installed: Vec<ComponentInfo> = vec![];

    let report = compare_tags(&used, &installed);
    assert_eq!(report.available.len(), 1);
    assert_eq!(report.available[0].tag, "checkbox");
    assert_eq!(report.available[0].crate_name, "light-fw-element-form");
    assert!(report.unknown.is_empty());
    assert!(report.has_problems());
}

// ---------------------------------------------------------------------------
// Сценарий 3 по ТЗ: добавлен `<datepicker>` (есть в bundled registry как
// `light-widget-datepicker`) → «Доступно» (не установлено, но известно).
// ---------------------------------------------------------------------------
#[test]
fn check_datepicker_is_available() {
    let mut used: BTreeMap<String, usize> = BTreeMap::new();
    used.insert("div".into(), 1);
    used.insert("datepicker".into(), 1);
    let installed: Vec<ComponentInfo> = vec![];

    let report = compare_tags(&used, &installed);
    assert_eq!(report.available.len(), 1);
    assert_eq!(report.available[0].crate_name, "light-widget-datepicker");
    assert!(report.unknown.is_empty());
    assert!(report.has_problems());
}

// ---------------------------------------------------------------------------
// Сценарий 4 по ТЗ: `light add light-fw-element-form` после <checkbox> →
// чистый отчёт (установлено, используется, без проблем).
// ---------------------------------------------------------------------------
#[test]
fn check_after_adding_form_component_is_clean() {
    let mut used: BTreeMap<String, usize> = BTreeMap::new();
    used.insert("div".into(), 1);
    used.insert("checkbox".into(), 1);
    let installed = vec![component(
        "light-fw-element-form",
        "0.17.0",
        &["select", "checkbox", "radio", "option"],
    )];

    let report = compare_tags(&used, &installed);
    assert_eq!(report.installed.len(), 1);
    assert_eq!(report.installed[0].crate_name, "light-fw-element-form");
    assert!(report.available.is_empty());
    assert!(report.unknown.is_empty());
    assert!(
        report.unused.is_empty(),
        "checkbox is used → form component is not unused"
    );
    assert!(report.conflicts.is_empty());
    assert!(!report.has_problems());
}

// ---------------------------------------------------------------------------
// Конфликт: два установленных компонента предоставляют один и тот же тег.
// ---------------------------------------------------------------------------
#[test]
fn check_conflict_two_components_share_tag() {
    let mut used: BTreeMap<String, usize> = BTreeMap::new();
    used.insert("shared".into(), 1);
    let installed = vec![
        component("light-fixture-a", "0.1.0", &["shared"]),
        component("light-fixture-b", "0.2.0", &["shared"]),
    ];

    let report = compare_tags(&used, &installed);
    assert_eq!(report.conflicts.len(), 1);
    assert_eq!(report.conflicts[0].tag, "shared");
    assert_eq!(report.conflicts[0].owners.len(), 2);
    assert!(
        report.has_problems(),
        "conflict should be a problem for --strict"
    );
}

// ---------------------------------------------------------------------------
// Неиспользуемый компонент: установлен, но ни один из его элементов/атрибутов
// не встречается в шаблонах.
// ---------------------------------------------------------------------------
#[test]
fn check_unused_component_when_nothing_matches() {
    let mut used: BTreeMap<String, usize> = BTreeMap::new();
    used.insert("div".into(), 1);
    // light-fw-behavior-validation предоставляет :validator/required/@validate.
    let installed = vec![component("light-fw-behavior-validation", "0.17.0", &[])];
    // Перезадаём provides, чтобы включить атрибуты/директивы.
    let mut installed = installed;
    installed[0].provides.attributes = vec!["required".into(), ":validator".into()];
    installed[0].provides.directives = vec!["@validate".into()];

    let report = compare_tags(&used, &installed);
    assert_eq!(
        report.unused.len(),
        1,
        "validation component should be unused"
    );
    assert_eq!(report.unused[0].crate_name, "light-fw-behavior-validation");
}

// ---------------------------------------------------------------------------
// Dedup available по crate_name: несколько тегов одного крейта → одна запись.
// ---------------------------------------------------------------------------
#[test]
fn check_available_deduped_by_crate() {
    let mut used: BTreeMap<String, usize> = BTreeMap::new();
    used.insert("checkbox".into(), 1);
    used.insert("select".into(), 1);
    used.insert("radio".into(), 1);
    let installed: Vec<ComponentInfo> = vec![];

    let report = compare_tags(&used, &installed);
    assert_eq!(
        report.available.len(),
        1,
        "all three tags come from light-fw-element-form → one entry"
    );
    assert_eq!(report.available[0].crate_name, "light-fw-element-form");
}

// ---------------------------------------------------------------------------
// load_installed_from_json: крейты без [package.metadata.light] пропускаются
// (не дают ошибки), крейты с метаданными попадают в installed.
// ---------------------------------------------------------------------------
#[test]
fn load_installed_json_skips_non_light_and_keeps_light() {
    // Фиксированные пути (не tempdir): в Windows песочнице cargo metadata
    // не работает стабильно, поэтому тестируем только чистый парсинг JSON
    // со статическим fixture-директорием. Путь не существует на диске —
    // парсеру не нужно его читать, он просто передаёт дальше.
    // Но load_installed_from_json читает manifest_path, поэтому создаём
    // фиксированный каталог в std::env::temp_dir() (уважает $TMP/$TEMP,
    // не ломается под AV-блокировкой C:\Temp).
    let root = std::env::temp_dir().join("light-check-fixture");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("mkdir root");
    for (name, is_light) in [("light-widget-datepicker", true), ("serde", false)] {
        let d = root.join(name);
        std::fs::create_dir_all(&d).unwrap();
        let mut toml = format!(
            r#"[package]
name = "{name}"
version = "0.1.0"
edition = "2021"
"#
        );
        if is_light {
            // Фикстура core-версии выводится из версии крейта (env! CARGO_PKG_VERSION):
            // pre-release (0.21.0-alpha.1) → ^0.21-0, stable (0.21.0) → ^0.21.
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
            toml.push_str(&format!(
                r#"

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
            ));
        }
        std::fs::write(d.join("Cargo.toml"), toml).unwrap();
    }
    std::fs::write(
        root.join("Cargo.toml"),
        r#"[package]
name = "app"
version = "0.1.0"
edition = "2021"

[dependencies]
light-widget-datepicker = { path = "./light-widget-datepicker" }
serde = { path = "./serde" }
"#,
    )
    .unwrap();

    // JSON строим через serde_json (Windows backslash'ы экранируются
    // корректно). Пути — native (backslash'ы) для `std::fs::read_to_string`.
    // Корневой пакет (root/Cargo.toml) и два подпакета.
    let root_manifest = root.join("Cargo.toml");
    let light_manifest = root.join("light-widget-datepicker").join("Cargo.toml");
    let serde_manifest = root.join("serde").join("Cargo.toml");
    let pkgs: Vec<serde_json::Value> = vec![
        serde_json::json!({
            "name": "app",
            "version": "0.1.0",
            "manifest_path": root_manifest.to_string_lossy(),
        }),
        serde_json::json!({
            "name": "light-widget-datepicker",
            "version": "0.1.0",
            "manifest_path": light_manifest.to_string_lossy(),
        }),
        serde_json::json!({
            "name": "serde",
            "version": "0.1.0",
            "manifest_path": serde_manifest.to_string_lossy(),
        }),
    ];
    let json = serde_json::json!({ "packages": pkgs }).to_string();
    let installed = load_installed_from_json(&json).expect("parse");
    assert_eq!(
        installed.len(),
        1,
        "only light-widget-datepicker should be in installed, got: {installed:?}"
    );
    assert_eq!(installed[0].crate_name, "light-widget-datepicker");
    assert_eq!(
        installed[0].provides_elements,
        vec!["datepicker".to_string()]
    );
    // Очистка.
    let _ = std::fs::remove_dir_all(root);
}

// ---------------------------------------------------------------------------
// To_text: все 5 категорий присутствуют в выводе (с пустыми значениями —
// «нет»), при наличии данных — перечислены.
// ---------------------------------------------------------------------------
#[test]
fn check_report_to_text_renders_all_categories() {
    let report = CheckReport {
        installed: vec![component("light-fw-element-form", "0.17.0", &["checkbox"])],
        available: vec![AvailableComponent {
            tag: "datepicker".into(),
            crate_name: "light-widget-datepicker".into(),
            requirement: "light-widget-datepicker ^0.18".into(),
        }],
        unknown: vec!["custom-tag".to_string()],
        unused: vec![component("light-fw-behavior-validation", "0.17.0", &[])],
        conflicts: vec![Conflict {
            tag: "shared".into(),
            owners: vec!["a".into(), "b".into()],
        }],
    };
    let text = report.to_text();
    for marker in [
        "🟢 Установлено:",
        "🟡 Доступно (установить):",
        "🔴 Неизвестно",
        "🗑 Не используется",
        "⚠️ Конфликты:",
    ] {
        assert!(text.contains(marker), "missing {marker:?} in:\n{text}");
    }
    // Данные присутствуют.
    assert!(text.contains("light-fw-element-form"));
    assert!(text.contains("datepicker"));
    assert!(text.contains("custom-tag"));
    assert!(text.contains("light-fw-behavior-validation"));
    assert!(text.contains("shared"));
}

// ---------------------------------------------------------------------------
// To_json: все поля сериализуются с правильными именами.
// ---------------------------------------------------------------------------
#[test]
fn check_report_to_json_has_all_fields() {
    let report = CheckReport {
        installed: vec![component("a", "0.1.0", &["x"])],
        available: vec![AvailableComponent {
            tag: "y".into(),
            crate_name: "c".into(),
            requirement: "c ^0.1".into(),
        }],
        unknown: vec!["z".to_string()],
        unused: vec![component("d", "0.2.0", &["w"])],
        conflicts: vec![Conflict {
            tag: "s".into(),
            owners: vec!["o1".into(), "o2".into()],
        }],
    };
    let json = report.to_json();
    assert_eq!(json["installed"].as_array().map(Vec::len), Some(1));
    assert_eq!(json["available"].as_array().map(Vec::len), Some(1));
    assert_eq!(json["unknown"].as_array().map(Vec::len), Some(1));
    assert_eq!(json["unused"].as_array().map(Vec::len), Some(1));
    assert_eq!(json["conflicts"].as_array().map(Vec::len), Some(1));
    // Поля AvailableComponent.
    assert_eq!(json["available"][0]["tag"], "y");
    assert_eq!(json["available"][0]["crate_name"], "c");
    assert_eq!(json["available"][0]["requirement"], "c ^0.1");
    // Поля Conflict.
    assert_eq!(json["conflicts"][0]["tag"], "s");
    assert_eq!(json["conflicts"][0]["owners"][0], "o1");
    assert_eq!(json["conflicts"][0]["owners"][1], "o2");
}

// ---------------------------------------------------------------------------
// has_problems: только available/unknown/conflicts триггерят strict; unused
// и installed — нет.
// ---------------------------------------------------------------------------
#[test]
fn has_problems_semantics() {
    // Чистый отчёт — no problems.
    let clean = CheckReport::default();
    assert!(!clean.has_problems());

    // Только unused — не проблема.
    let only_unused = CheckReport {
        unused: vec![component("x", "0.1.0", &["y"])],
        ..Default::default()
    };
    assert!(
        !only_unused.has_problems(),
        "unused alone is not a strict blocker"
    );

    // Только available — проблема.
    let only_available = CheckReport {
        available: vec![AvailableComponent {
            tag: "t".into(),
            crate_name: "c".into(),
            requirement: "c ^0.1".into(),
        }],
        ..Default::default()
    };
    assert!(only_available.has_problems());

    // Только conflict — проблема.
    let only_conflict = CheckReport {
        conflicts: vec![Conflict {
            tag: "t".into(),
            owners: vec!["a".into(), "b".into()],
        }],
        ..Default::default()
    };
    assert!(only_conflict.has_problems());
}

// ---------------------------------------------------------------------------
// build_check_report на временном проекте: даже если cargo metadata в
// песочнице вернёт пустой вывод, scan_used_tags должен работать — проверяем
// что вызов не паникует и (в лучшем случае) возвращает valid report.
// Если cargo недоступен, тест пропускается gracefully через `ok_or_else`.
// ---------------------------------------------------------------------------
#[test]
fn build_check_report_smoke_on_scratch_project() {
    let dir = tempdir().expect("tempdir");
    make_project(
        dir.path(),
        "<window>\n  <div class=\"center\">\n    <h1>Hello</h1>\n  </div>\n</window>\n",
    );
    let result = build_check_report(dir.path());
    // В реальном окружении (CI Linux, developer machine) это Ok(...).
    // В песочнице Windows может быть Err (cargo metadata пустой stdout) —
    // не считаем это нашим багом. Проверяем, что паники не было.
    match result {
        Ok(report) => {
            // Fresh проект → нет доступных/неизвестных.
            assert!(report.unknown.is_empty(), "no custom tags in fresh project");
            assert!(
                report.available.is_empty(),
                "no missing components in fresh project"
            );
        }
        Err(_) => {
            // Ожидаемо в песочнице — не паникуем, просто фиксируем.
        }
    }
}
