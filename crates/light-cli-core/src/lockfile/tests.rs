#[allow(clippy::module_inception)]
#[cfg(test)]
mod tests {
    use crate::lockfile::*;
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::PathBuf;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("light-fw-cli-lock-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    /// Создаёт минимальный lock-файл без компонентов.
    fn empty_lock() -> Lockfile {
        Lockfile {
            version: 1,
            core: "0.1.0".to_string(),
            components: Vec::new(),
            plugins: BTreeMap::new(),
        }
    }

    #[test]
    fn toml_round_trip() {
        let lock = Lockfile {
            version: 1,
            core: "0.1.0".to_string(),
            plugins: BTreeMap::new(),
            components: vec![LockComponent {
                name: "light-fw-element-form".to_string(),
                version: "0.1.0".to_string(),
                source: "workspace".to_string(),
                path: Some("crates/light-element-form".to_string()),
                dependencies: vec![LockDependency {
                    name: "light-fw-core".to_string(),
                    version: "0.1.0".to_string(),
                }],
            }],
        };
        let text = lock.to_toml().expect("to_toml");
        assert!(text.contains("not edit") || text.contains("не редактировать"));
        let parsed = Lockfile::from_toml(&text).expect("from_toml");
        assert_eq!(parsed, lock);
    }

    #[test]
    fn resolves_core_from_workspace_meta_crate() {
        let root = temp_dir("core-workspace");
        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"app\", \"light\"]\nresolver = \"2\"\n\n\
             [workspace.package]\nversion = \"0.2.3\"\n\n\
             [workspace.dependencies]\nlight = { path = \"light\" }\n",
        )
        .expect("root manifest");
        let app = root.join("app");
        fs::create_dir_all(&app).expect("app");
        fs::write(
            app.join("Cargo.toml"),
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
             [dependencies]\nlight = { workspace = true }\n",
        )
        .expect("app manifest");
        let light = root.join("light");
        fs::create_dir_all(light.join("src")).expect("light src");
        fs::write(light.join("src/lib.rs"), "").expect("light lib");
        fs::write(
            light.join("Cargo.toml"),
            "[package]\nname = \"light\"\nversion.workspace = true\nedition = \"2021\"\n",
        )
        .expect("light manifest");

        let lock = Lockfile::from_project(&app).expect("from_project");
        assert_eq!(lock.core, "0.2.3", "версия ядра из workspace");
        assert!(lock.components.is_empty());
    }

    #[test]
    fn from_project_reads_components() {
        let dir = temp_dir("from-project");
        fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname=\"p\"\nversion=\"0.1.0\"\n\n[dependencies]\nlight = \"0.1\"\nlight-fw-element-form = \"0.1\"\n",
        )
        .expect("write");
        let lock = Lockfile::from_project(&dir).expect("from_project");
        assert_eq!(lock.version, 1);
        assert_eq!(lock.components.len(), 1);
        assert_eq!(lock.components[0].name, "light-fw-element-form");
        assert_eq!(lock.core, "0.1.0");
    }

    #[test]
    fn from_project_includes_external_component() {
        let dir = temp_dir("external");
        fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname=\"p\"\nversion=\"0.1.0\"\n\n[dependencies]\nlight = \"0.1\"\n\
             light-fw-datepicker = { version = \"0.3.0\", optional = true }\n",
        )
        .expect("write");
        let lock = Lockfile::from_project(&dir).expect("from_project");
        assert_eq!(lock.components.len(), 1);
        assert_eq!(lock.components[0].name, "light-fw-datepicker");
        assert_eq!(lock.components[0].version, "0.3.0");
        assert_eq!(lock.components[0].source, "crates.io");
    }

    #[test]
    fn is_actual_detects_drift() {
        let dir = temp_dir("actual");
        fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname=\"p\"\nversion=\"0.1.0\"\n\n[dependencies]\nlight = \"0.1\"\nlight-fw-element-form = \"0.1\"\n",
        )
        .expect("write");
        // Нет файла → не актуален.
        assert!(!is_actual(&dir).expect("is_actual"));
        // Записываем корректный lock → актуален.
        Lockfile::from_project(&dir)
            .expect("build")
            .write(&dir)
            .expect("write lock");
        assert!(is_actual(&dir).expect("is_actual"));
        // Меняем Cargo.toml → снова не актуален.
        fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname=\"p\"\nversion=\"0.1.0\"\n\n[dependencies]\nlight = \"0.1\"\n",
        )
        .expect("write");
        assert!(!is_actual(&dir).expect("is_actual"));
    }

    #[test]
    fn diff_reports_added_removed_changed() {
        let base = Lockfile {
            version: 1,
            core: "0.1.0".to_string(),
            plugins: BTreeMap::new(),
            components: vec![
                component("light-fw-element-form", "0.1.0"),
                component("light-fw-behavior-validation", "0.1.0"),
            ],
        };
        let updated = Lockfile {
            version: 1,
            core: "0.1.0".to_string(),
            plugins: BTreeMap::new(),
            components: vec![
                component("light-fw-element-form", "0.2.0"),
                component("light-widget-datepicker", "0.3.0"),
            ],
        };
        let changes = diff(&base, &updated);
        assert_eq!(changes.changed, vec!["light-fw-element-form"]);
        assert_eq!(changes.added, vec!["light-widget-datepicker"]);
        assert_eq!(changes.removed, vec!["light-fw-behavior-validation"]);
        assert!(!changes.is_empty());
        assert!(changes.to_text().contains("~ light-fw-element-form"));
        assert!(diff(&base, &base).is_empty());
    }

    fn component(name: &str, version: &str) -> LockComponent {
        LockComponent {
            name: name.to_string(),
            version: version.to_string(),
            source: "workspace".to_string(),
            path: None,
            dependencies: Vec::new(),
        }
    }

    #[test]
    fn set_plugin_priority_inserts_and_overwrites() {
        let mut lock = empty_lock();
        lock.set_plugin_priority("datepicker", "light-fw-datepicker");
        assert_eq!(
            lock.plugin_priority("datepicker"),
            Some("light-fw-datepicker")
        );
        // Повторный вызов перезаписывает победителя.
        lock.set_plugin_priority("datepicker", "light-widget-datepicker");
        assert_eq!(
            lock.plugin_priority("datepicker"),
            Some("light-widget-datepicker")
        );
        // Незафиксированный тег — None.
        assert_eq!(lock.plugin_priority("color-picker"), None);
    }

    #[test]
    fn old_lockfile_without_plugins_parses_empty() {
        // Обратная совместимость: файл без секции [plugins] разбирается как
        // пустой набор приоритетов, сериализуется валидно.
        let text = "# light.lock\nversion = 1\ncore = \"0.1.0\"\n\n\
                    [[component]]\nname = \"light-fw-element-form\"\n\
                    version = \"0.1.0\"\nsource = \"crates.io\"\n";
        let lock = Lockfile::from_toml(text).expect("from_toml");
        assert!(lock.plugins.is_empty(), "старый файл → нет приоритетов");
        assert_eq!(lock.components.len(), 1);
        // После записи секция [plugins] появляется, но пуста (томli-edit
        // пропускает пустые таблицы), round-trip стабилен.
        let out = lock.to_toml().expect("to_toml");
        let reparsed = Lockfile::from_toml(&out).expect("reparse");
        assert_eq!(reparsed, lock);
    }

    #[test]
    fn plugins_serialize_to_named_keys() {
        let mut lock = empty_lock();
        lock.set_plugin_priority("datepicker", "light-widget-datepicker");
        let text = lock.to_toml().expect("to_toml");
        // Ключ-тег = значение-крейт внутри секции [plugins].
        assert!(text.contains("[plugins]"), "отсутствует [plugins]\n{text}");
        assert!(
            text.contains("datepicker = \"light-widget-datepicker\""),
            "нет пары тег→крейт\n{text}"
        );
        let parsed = Lockfile::from_toml(&text).expect("from_toml");
        assert_eq!(parsed, lock);
    }

    #[test]
    fn from_project_preserves_plugins_from_existing_lock() {
        // Приоритеты не выводятся из Cargo.toml — они переносятся из
        // существующего light.lock при регенерации.
        let dir = temp_dir("preserve-plugins");
        fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname=\"p\"\nversion=\"0.1.0\"\n\n[dependencies]\nlight = \"0.1\"\n",
        )
        .expect("write");
        // Записываем lock с одним приоритетом.
        let mut seed = Lockfile::from_project(&dir).expect("seed");
        seed.set_plugin_priority("datepicker", "light-widget-datepicker");
        seed.write(&dir).expect("write seed lock");
        // Регенерация из Cargo.toml должна сохранить приоритет.
        let regenerated = Lockfile::from_project(&dir).expect("regen");
        assert_eq!(
            regenerated.plugin_priority("datepicker"),
            Some("light-widget-datepicker"),
            "приоритет должен переноситься из light.lock"
        );
    }

    #[test]
    fn template_count_works() {
        let dir = temp_dir("count");
        fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname=\"p\"\nversion=\"0.1.0\"\n",
        )
        .expect("write");
        assert_eq!(template_count(&dir).expect("count"), 0);
    }
}
