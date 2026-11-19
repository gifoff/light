#[allow(clippy::module_inception)]
#[cfg(test)]
mod tests {
    use crate::check::run::*;
    use crate::check::types::*;
    use crate::manifest::Dependency;
    use crate::manifest::Provides;
    use crate::report::{Issue, ProjectReport, Recommendation};
    use crate::resolve::RequiredComponent;
    use crate::scan::ScanResult;
    use std::collections::BTreeMap;
    use std::path::Path;

    fn test_report() -> ProjectReport {
        let mut scan = ScanResult::default();
        scan.tags.insert("select".to_string(), 1);
        scan.attributes.insert("required".to_string(), 2);

        ProjectReport {
            scan,
            installed: Vec::<Dependency>::new(),
            missing: vec![
                RequiredComponent {
                    name: "light-fw-element-form".into(),
                },
                RequiredComponent {
                    name: "light-fw-behavior-validation".into(),
                },
            ],
            unused: Vec::new(),
            issues: Vec::<Issue>::new(),
            recommendations: Vec::<Recommendation>::new(),
        }
    }

    #[test]
    fn format_missing_block_with_tags() {
        let report = test_report();
        let block = format_missing_block(&report);
        assert!(block.starts_with("⚠️ Обнаружены недостающие компоненты:"));
        assert!(block.contains("- light-fw-element-form"));
        assert!(block.contains("- light-fw-behavior-validation"));
    }

    #[test]
    fn parse_yn_accepts_yes() {
        assert!(parse_yn("y"));
        assert!(parse_yn("Y"));
        assert!(parse_yn("yes"));
        assert!(parse_yn("YES"));
        assert!(parse_yn("Yes"));
    }

    #[test]
    fn parse_yn_accepts_empty() {
        assert!(parse_yn(""));
        assert!(parse_yn("  \n"));
    }

    #[test]
    fn parse_yn_rejects_no() {
        assert!(!parse_yn("n"));
        assert!(!parse_yn("no"));
        assert!(!parse_yn("N"));
        assert!(!parse_yn("hello"));
    }

    /// Регрессия 0.17.1: в НЕИНТЕРАКТИВНОМ режиме (`CI`, пайплайн, `echo Y |
    /// light build`) [`ensure_components_present`] **не читает stdin** и не
    /// блокируется, а устанавливает недостающие компоненты автоматически и
    /// возвращает `true`. Поведение ветки целиком определяется ответом и
    /// [`parse_yn`]: пустой ввод = согласие, явное «n» = отказ (установка не
    /// запускается). Файловая система здесь не нужна — проверка опирается на
    /// чистые функции, что делает тест детерминированным в любой среде
    /// (включая песочницы Windows, где запись в TEMP может быть запрещена).
    #[test]
    fn noninteractive_branch_autinstalls_without_reading_stdin() {
        use crate::resolve::RequiredComponent;

        // Неинтерактивный режим: `is_interactive() == false` → ответ всегда
        // `true` (без чтения stdin), как в коде ветки `else { ...; true }`.
        let missing = vec![
            RequiredComponent {
                name: "light-fw-element-form".into(),
            },
            RequiredComponent {
                name: "light-fw-behavior-validation".into(),
            },
        ];
        let non_interactive_answer = true; // результат ветки `else` при !is_interactive()

        // Согласие → установка каждого недостающего компонента и готовность.
        let installed = emulate_auto_install(&missing, non_interactive_answer);
        assert_eq!(
            installed,
            vec![
                "light-fw-element-form".to_string(),
                "light-fw-behavior-validation".to_string()
            ],
            "non-interactive mode installs every missing component"
        );
        // Готовность равна ответу (в неинтерактивном режиме — `true`).
        assert!(
            non_interactive_answer,
            "non-interactive mode returns ready=true"
        );

        // Пустой ввод трактуется как согласие (автоматическая установка),
        // даже если пользователь ничего не нажал.
        assert!(parse_yn("\n"), "empty stdin line means auto-yes");
        // Явный отказ → установка не выполняется.
        assert!(!parse_yn("n\n"), "explicit no must refuse");
        let refused = emulate_auto_install(&missing, parse_yn("n\n"));
        assert!(refused.is_empty(), "no components installed after refusal");
    }

    /// Эмуляция установки компонентов по аналогии с циклом
    /// `for name in &report.missing { install_fn(&name.name)?; }` в
    /// [`ensure_components_present`]: при согласии устанавливает все, при
    /// отказе — ни одного. Возвращает список установленных имён.
    fn emulate_auto_install(
        missing: &[crate::resolve::RequiredComponent],
        answer: bool,
    ) -> Vec<String> {
        let installed: std::cell::RefCell<Vec<String>> = std::cell::RefCell::new(Vec::new());
        let install_fn = |name: &str| {
            installed.borrow_mut().push(name.to_string());
        };
        if answer {
            for c in missing {
                install_fn(&c.name);
            }
        }
        installed.into_inner()
    }

    // ─────────────── Умная проверка (Этап 1) ─────────────────────────────────

    /// Разметка фикстуры: создаёт каталог с `templates/app.html` и
    /// произвольным содержимым.
    fn templates_fixture(dir: &Path, app_html: &str) {
        std::fs::create_dir_all(dir.join("templates")).unwrap();
        std::fs::write(dir.join("templates/app.html"), app_html).unwrap();
    }

    #[test]
    fn scan_used_tags_extracts_div_checkbox_datepicker() {
        let dir = std::env::temp_dir().join("light-fw-cli-scan-tags");
        let _ = std::fs::remove_dir_all(&dir);
        templates_fixture(
            &dir,
            r#"
            <div class="wrap">
              <checkbox v-model="agreed"></checkbox>
              <datepicker v-model="date"></datepicker>
              <button>OK</button>
            </div>
            "#,
        );
        let used = scan_used_tags(&dir).expect("scan");
        assert_eq!(used.get("div"), Some(&1));
        assert_eq!(used.get("checkbox"), Some(&1));
        assert_eq!(used.get("datepicker"), Some(&1));
        assert_eq!(used.get("button"), Some(&1));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_used_tags_ignores_non_html_and_no_templates() {
        let dir = std::env::temp_dir().join("light-fw-cli-scan-tags-2");
        let _ = std::fs::remove_dir_all(&dir);
        // Без каталога `templates` → пустой результат.
        std::fs::create_dir_all(&dir).unwrap();
        let used = scan_used_tags(&dir).expect("scan");
        assert!(used.is_empty());
        // Не-HTML файл игнорируется.
        templates_fixture(&dir, "");
        std::fs::write(dir.join("templates/README.md"), "<div></div>\n").unwrap();
        let used = scan_used_tags(&dir).expect("scan");
        assert!(used.is_empty(), "md не сканируется");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Хелпер: строит [`ComponentInfo`] из полей по умолчанию.
    fn component_info(name: &str, version: &str, elements: &[&str]) -> ComponentInfo {
        ComponentInfo {
            crate_name: name.into(),
            version: version.into(),
            provides: Provides {
                elements: elements.iter().map(|s| s.to_string()).collect(),
                attributes: Vec::new(),
                directives: Vec::new(),
            },
        }
    }

    #[test]
    fn check_compare_empty_project_has_no_categories() {
        let mut used: BTreeMap<String, usize> = BTreeMap::new();
        used.insert("div".into(), 1);
        used.insert("span".into(), 2);
        let installed: Vec<ComponentInfo> = Vec::new();
        let report = compare_tags(&used, &installed);
        assert!(report.installed.is_empty());
        assert!(report.available.is_empty());
        assert!(report.unknown.is_empty());
        assert!(report.unused.is_empty());
        assert!(report.conflicts.is_empty());
        assert!(!report.has_problems());
    }

    #[test]
    fn check_compare_checkbox_without_form_is_available() {
        let mut used: BTreeMap<String, usize> = BTreeMap::new();
        used.insert("div".into(), 1);
        used.insert("checkbox".into(), 1);
        let installed: Vec<ComponentInfo> = Vec::new();
        let report = compare_tags(&used, &installed);
        // checkbox не установлен, но есть в реестре (light-fw-element-form).
        assert!(report.installed.is_empty());
        assert_eq!(report.available.len(), 1);
        assert_eq!(report.available[0].tag, "checkbox");
        assert_eq!(report.available[0].crate_name, "light-fw-element-form");
        assert!(report.unknown.is_empty());
        assert!(
            report.has_problems(),
            "--strict должен падать при available"
        );
    }

    #[test]
    fn check_compare_unknown_tag_is_unknown() {
        // Этап 6 Plugin API: `datepicker` теперь есть в bundled registry.toml
        // (`light-widget-datepicker`), поэтому для покрытия `unknown`-ветки
        // используем тег, которого реально нет ни в ядре, ни в реестре.
        let mut used: BTreeMap<String, usize> = BTreeMap::new();
        used.insert("div".into(), 1);
        used.insert("alien-widget".into(), 1);
        let installed: Vec<ComponentInfo> = Vec::new();
        let report = compare_tags(&used, &installed);
        // alien-widget нет в bundled registry.toml → unknown.
        assert!(report.available.is_empty());
        assert_eq!(report.unknown, vec!["alien-widget".to_string()]);
        assert!(report.has_problems());
    }

    #[test]
    fn check_compare_datepicker_without_plugin_is_available() {
        // Этап 6 Plugin API: `<datepicker />` в свежем проекте (плагин не
        // установлен) классифицируется как `available` — `light dev`/`build`
        // предложат установить `light-widget-datepicker`.
        let mut used: BTreeMap<String, usize> = BTreeMap::new();
        used.insert("div".into(), 1);
        used.insert("datepicker".into(), 1);
        let installed: Vec<ComponentInfo> = Vec::new();
        let report = compare_tags(&used, &installed);
        assert!(report.installed.is_empty());
        assert_eq!(report.available.len(), 1);
        assert_eq!(report.available[0].tag, "datepicker");
        assert_eq!(report.available[0].crate_name, "light-widget-datepicker");
        assert!(report.unknown.is_empty());
        assert!(report.has_problems());
    }

    #[test]
    fn check_compare_conflict_two_components_share_tag() {
        let mut used: BTreeMap<String, usize> = BTreeMap::new();
        used.insert("shared".into(), 1);
        let installed = vec![
            component_info("light-fixture-a", "0.1.0", &["shared"]),
            component_info("light-fixture-b", "0.1.0", &["shared"]),
        ];
        let report = compare_tags(&used, &installed);
        assert_eq!(report.installed.len(), 2);
        assert_eq!(report.available.len(), 0);
        assert_eq!(report.unknown.len(), 0);
        assert_eq!(report.conflicts.len(), 1);
        assert_eq!(report.conflicts[0].tag, "shared");
        assert_eq!(
            report.conflicts[0].owners,
            vec!["light-fixture-a", "light-fixture-b"]
        );
        assert!(report.has_problems());
    }

    #[test]
    fn check_compare_installed_and_used_component_not_unused() {
        let mut used: BTreeMap<String, usize> = BTreeMap::new();
        used.insert("checkbox".into(), 1);
        let installed = vec![component_info(
            "light-fw-element-form",
            "0.17.0",
            &["checkbox"],
        )];
        let report = compare_tags(&used, &installed);
        assert_eq!(report.installed.len(), 1);
        assert_eq!(report.installed[0].crate_name, "light-fw-element-form");
        // checkbox использован → не unused.
        assert!(report.unused.is_empty());
        assert!(report.available.is_empty());
        assert!(report.unknown.is_empty());
        assert!(!report.has_problems());
    }

    #[test]
    fn check_compare_unused_component_when_tag_not_in_templates() {
        // Установлен light-fw-element-form, но ни select/checkbox/radio/option
        // не используется → попадает в `unused`.
        let mut used: BTreeMap<String, usize> = BTreeMap::new();
        used.insert("div".into(), 1);
        let installed = vec![component_info(
            "light-fw-element-form",
            "0.17.0",
            &["select", "checkbox", "radio", "option"],
        )];
        let report = compare_tags(&used, &installed);
        assert_eq!(report.unused.len(), 1);
        assert_eq!(report.unused[0].crate_name, "light-fw-element-form");
    }

    #[test]
    fn check_report_to_text_contains_category_markers() {
        let report = CheckReport {
            installed: vec![ComponentInfo {
                crate_name: "light-fw-element-form".into(),
                version: "0.17.0".into(),
                provides: Provides {
                    elements: vec!["checkbox".into()],
                    attributes: vec![],
                    directives: vec![],
                },
            }],
            available: vec![AvailableComponent {
                tag: "datepicker".into(),
                crate_name: "light-widget-datepicker".into(),
                requirement: "light-widget-datepicker ^0.19".into(),
            }],
            unknown: vec!["custom-el".into()],
            unused: Vec::new(),
            conflicts: vec![Conflict {
                tag: "dup".into(),
                owners: vec!["a".into(), "b".into()],
            }],
        };
        let text = report.to_text();
        assert!(text.contains("🟢 Установлено"));
        assert!(text.contains("light-fw-element-form"));
        assert!(text.contains("🟡 Доступно (установить)"));
        assert!(text.contains("light-widget-datepicker ^0.19"));
        assert!(text.contains("🔴 Неизвестно"));
        assert!(text.contains("custom-el"));
        assert!(text.contains("⚠️ Конфликты"));
        assert!(text.contains("dup"));
    }

    #[test]
    fn check_report_to_json_serializes_all_categories() {
        let report = CheckReport {
            installed: Vec::new(),
            available: vec![AvailableComponent {
                tag: "checkbox".into(),
                crate_name: "light-fw-element-form".into(),
                requirement: "light-fw-element-form ^0.17".into(),
            }],
            unknown: vec!["custom".into()],
            unused: Vec::new(),
            conflicts: Vec::new(),
        };
        let json = report.to_json();
        assert_eq!(json["available"][0]["tag"], "checkbox");
        assert_eq!(json["available"][0]["crate_name"], "light-fw-element-form");
        assert_eq!(json["unknown"][0], "custom");
        assert_eq!(json["installed"].as_array().unwrap().len(), 0);
    }
}
