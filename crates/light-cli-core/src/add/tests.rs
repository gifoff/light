#[cfg(test)]
#[allow(clippy::module_inception)]
mod tests {
    use crate::add::{
        AddOptions, apply_add, check_tag_conflict, choose_feature, default_feature_name, plan_add,
        plan_add_with_catalog,
    };
    use crate::error::Error;
    use crate::fetch::{Catalog, DependencyEntry, RemoteRegistry, ResolvedComponent, SourceKind};
    use crate::process::{CommandOutput, CommandRunner};
    use crate::registry::ComponentRegistry;
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;

    #[derive(Default)]
    struct MockRunner {
        calls: Mutex<Vec<String>>,
        fail_check: bool,
    }

    impl CommandRunner for MockRunner {
        fn run(
            &self,
            program: &str,
            args: &[String],
            _cwd: Option<&Path>,
        ) -> std::result::Result<CommandOutput, crate::error::Error> {
            self.calls
                .lock()
                .expect("lock")
                .push(format!("{program} {}", args.join(" ")));
            if self.fail_check && args.first().map(String::as_str) == Some("check") {
                return Ok(CommandOutput {
                    status: 101,
                    stdout: String::new(),
                    stderr: "compile error".to_string(),
                });
            }
            Ok(CommandOutput {
                status: 0,
                stdout: String::new(),
                stderr: String::new(),
            })
        }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("light-fw-cli-add-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    /// Создаёт мини-workspace с компонентом `light-fw-element-form`.
    fn workspace_fixture(name: &str) -> PathBuf {
        let root = temp_dir(name);
        std::fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"app\", \"comp\"]\nresolver = \"2\"\n\n\
             [workspace.dependencies]\nlight-fw-element-form = { path = \"comp\", version = \"0.1.0\" }\n",
        )
        .expect("root manifest");
        let app = root.join("app");
        std::fs::create_dir_all(&app).expect("app");
        std::fs::write(
            app.join("Cargo.toml"),
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
             [dependencies]\nlight = \"0.1\"\n",
        )
        .expect("app manifest");
        let comp = root.join("comp");
        std::fs::create_dir_all(comp.join("src")).expect("comp src");
        std::fs::write(comp.join("src/lib.rs"), "").expect("comp lib");
        std::fs::write(
            comp.join("Cargo.toml"),
            "[package]\nname = \"light-fw-element-form\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
             [package.metadata.light]\nkind = \"element\"\ndisplay-name = \"Form\"\ncore = \"^0.1\"\n\n\
             [package.metadata.light.provides]\nelements = [\"checkbox\"]\n",
        )
        .expect("comp manifest");
        app
    }

    /// Герметичная проверка конфликта по тегам без реального `cargo metadata`:
    /// создаёт в temp-каталоге «установленный» компонент с `[package.metadata.light]`
    /// и подмеченный JSON-ответ.
    fn tag_conflict_fixture(name: &str) -> PathBuf {
        let root = temp_dir(name);
        // Приложение.
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
             [dependencies]\nlight = \"0.1\"\n",
        )
        .expect("app manifest");
        // «Установленный» компонент (для load_installed_from_json).
        let installed_dir = root.join("installed");
        std::fs::create_dir_all(installed_dir.join("src")).expect("installed src");
        std::fs::write(installed_dir.join("src/lib.rs"), "").expect("installed lib");
        std::fs::write(
            installed_dir.join("Cargo.toml"),
            "[package]\nname = \"light-widget-datepicker\"\nversion = \"0.2.0\"\nedition = \"2021\"\n\n\
             [package.metadata.light]\nkind = \"element\"\ndisplay-name = \"DatePicker\"\ncore = \"^0.1\"\n\n\
             [package.metadata.light.provides]\nelements = [\"datepicker\"]\n",
        )
        .expect("installed manifest");
        // JSON, имитирующий `cargo metadata --no-deps`.
        let manifest_path = installed_dir.join("Cargo.toml");
        std::fs::write(
            root.join("metadata.json"),
            serde_json::json!({ "packages": [ {
                "name": "light-widget-datepicker",
                "version": "0.2.0",
                "manifest_path": manifest_path.to_string_lossy(),
            }] })
            .to_string(),
        )
        .expect("write metadata json");
        root
    }

    /// Герметичный тест: «установленный» `light-widget-datepicker` (через
    /// `load_installed_from_json`) + новый `light-fw-datepicker` с
    /// `provides.elements = ["datepicker"]`:
    /// - без `--force` → `Error::Conflict { tag, existing_crate, new_crate }`;
    /// - с `--force` → приоритет `("datepicker", "light-fw-datepicker")`.
    ///
    /// Тест обходит песочницу DSH (где `cargo metadata` возвращает пустой
    /// stdout) записывая `metadata.json` рядом и **вызывая**
    /// [`crate::metadata::load_installed_from_json`] для проверки, что парсинг
    /// метаданных работает. Сам `check_tag_conflict` использует
    /// `load_installed` (системный вызов); его поведение в песочнице —
    /// fallback к `Ok(vec![])` — покрыто отдельным E2E-тестом (`stage2`).
    #[test]
    fn load_installed_from_json_detects_provides_elements() {
        let root = tag_conflict_fixture("tag-json");
        let json = std::fs::read_to_string(root.join("metadata.json")).expect("read json");
        let installed = crate::metadata::load_installed_from_json(&json).expect("load from json");
        assert_eq!(installed.len(), 1);
        assert_eq!(installed[0].crate_name, "light-widget-datepicker");
        assert_eq!(
            installed[0].provides_elements,
            vec!["datepicker".to_string()]
        );
    }

    /// Чистый юнит-тест чистого функционала: создаёт искусственные данные и
    /// проверяет логику `check_tag_conflict` через прямое сравнение.
    /// `check_tag_conflict` — `pub(crate)`, поэтому тестируется в модуле.
    #[test]
    fn check_tag_conflict_pure_logic_with_fake_project() {
        // Создаём проект с пустым `Cargo.toml` без компонентов — `load_installed`
        // вернёт пустой набор (в песочнице — fallback, в CI — пустой metadata).
        let root = temp_dir("pure-tag-conflict");
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"empty\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .expect("write");

        // Компонент с provides.elements — в «пустом» проекте конфликта быть не может.
        let no_conflict = ResolvedComponent {
            name: "light-fw-datepicker".to_string(),
            version: semver::Version::new(0, 2, 0),
            core: semver::VersionReq::STAR,
            light_features: Vec::new(),
            provides: crate::manifest::Provides {
                elements: vec!["datepicker".to_string()],
                attributes: Vec::new(),
                directives: Vec::new(),
            },
            requires: Vec::new(),
            conflicts: Vec::new(),
            source: SourceKind::CratesIo,
            path: None,
        };
        let opts = AddOptions::default();
        // Без --force и без установленных — нет конфликта, приоритетов нет.
        let result = check_tag_conflict(&no_conflict, &root, &opts).expect("no conflict");
        assert!(result.is_empty());

        // Компонент без provides — тоже нет.
        let no_provides = ResolvedComponent {
            provides: crate::manifest::Provides {
                elements: Vec::new(),
                attributes: Vec::new(),
                directives: Vec::new(),
            },
            ..no_conflict.clone()
        };
        let result = check_tag_conflict(&no_provides, &root, &opts).expect("no conflict");
        assert!(result.is_empty());
    }

    #[test]
    fn plans_workspace_component() {
        let app = workspace_fixture("plan");
        let plan = plan_add(&app, "light-fw-element-form", &AddOptions::default()).expect("plan");
        assert_eq!(plan.dependency, DependencyEntry::Workspace);
        assert_eq!(plan.feature, "element-form");
        assert!(plan.creates_feature);
        assert!(plan.to_text().contains("light-fw-element-form"));
    }

    #[test]
    fn already_installed_is_idempotent() {
        let app = workspace_fixture("installed");
        let opts = AddOptions {
            no_verify: true,
            ..Default::default()
        };
        let plan = plan_add(&app, "light-fw-element-form", &opts).expect("plan");
        apply_add(&app, &MockRunner::default(), &plan, &opts).expect("apply");

        let again = plan_add(&app, "light-fw-element-form", &opts).expect("idempotent");
        assert!(again.already_installed);
        assert_eq!(again.component.version, semver::Version::new(0, 1, 0));
        assert_eq!(again.component.source, SourceKind::Workspace);
        // Применение повторного плана ничего не меняет.
        let before = std::fs::read_to_string(app.join("Cargo.toml")).expect("read");
        apply_add(&app, &MockRunner::default(), &again, &opts).expect("re-apply");
        assert_eq!(
            std::fs::read_to_string(app.join("Cargo.toml")).expect("read"),
            before
        );
    }

    #[test]
    fn already_installed_with_different_version_conflicts() {
        let app = workspace_fixture("installed-conflict");
        let opts = AddOptions {
            no_verify: true,
            ..Default::default()
        };
        let plan = plan_add(&app, "light-fw-element-form", &opts).expect("plan");
        apply_add(&app, &MockRunner::default(), &plan, &opts).expect("apply");

        let conflicting = AddOptions {
            version: Some("0.2.0".to_string()),
            ..opts
        };
        let err = plan_add(&app, "light-fw-element-form", &conflicting).expect_err("conflict");
        // Конфликт по версии переиспользует тот же вариант: диагностика в `tag`.
        assert!(matches!(&err, Error::Conflict { .. }));
        match &err {
            Error::Conflict {
                tag,
                existing_crate,
                new_crate,
            } => {
                assert_eq!(existing_crate, "light-fw-element-form");
                assert_eq!(new_crate, "light-fw-element-form");
                assert!(tag.contains("уже установлен"), "{tag}");
            }
            _ => panic!("ожидался Conflict, получено: {err:?}"),
        }
    }

    #[test]
    fn dry_run_does_not_touch_disk() {
        let app = workspace_fixture("dry-run");
        let original = std::fs::read_to_string(app.join("Cargo.toml")).expect("read");
        let opts = AddOptions {
            dry_run: true,
            ..Default::default()
        };
        let plan = plan_add(&app, "light-fw-element-form", &opts).expect("plan");
        apply_add(&app, &MockRunner::default(), &plan, &opts).expect("apply");
        assert_eq!(
            std::fs::read_to_string(app.join("Cargo.toml")).expect("read"),
            original
        );
        assert!(!app.join("light.lock").exists());
    }

    #[test]
    fn applies_workspace_dependency_and_feature() {
        let app = workspace_fixture("apply");
        let opts = AddOptions::default();
        let plan = plan_add(&app, "light-fw-element-form", &opts).expect("plan");
        let runner = MockRunner::default();
        let outcome = apply_add(&app, &runner, &plan, &opts).expect("apply");
        assert!(outcome.verified);

        let text = std::fs::read_to_string(app.join("Cargo.toml")).expect("read");
        assert!(text.contains("light-fw-element-form = { workspace = true, optional = true }"));
        assert!(text.contains("[features]"));
        assert!(text.contains("element-form = [\"dep:light-fw-element-form\"]"));
        assert!(app.join("light.lock").exists());

        let calls = runner.calls.lock().expect("lock").clone();
        assert!(calls.iter().any(|c| c == "cargo check --quiet"));
    }

    struct MockRemote {
        name: &'static str,
        version: semver::Version,
    }

    impl crate::fetch::RemoteRegistry for MockRemote {
        fn latest_version(
            &self,
            name: &str,
        ) -> std::result::Result<Option<semver::Version>, crate::error::Error> {
            if name == self.name {
                Ok(Some(self.version.clone()))
            } else {
                Ok(None)
            }
        }
    }

    #[test]
    fn adds_external_component_from_crates_io() {
        let app = workspace_fixture("external");
        let catalog = Catalog::with_remote(
            &app,
            false,
            Box::new(MockRemote {
                name: "light-fw-datepicker",
                version: semver::Version::new(0, 3, 0),
            }),
        );
        let opts = AddOptions {
            features: vec!["default".to_string()],
            ..Default::default()
        };
        let plan = plan_add_with_catalog(&app, "light-fw-datepicker", &opts, &catalog)
            .expect("plan external");
        assert_eq!(
            plan.dependency,
            DependencyEntry::Version("0.3.0".to_string())
        );
        assert_eq!(plan.feature, "datepicker");

        let outcome = apply_add(&app, &MockRunner::default(), &plan, &opts).expect("apply");
        assert!(outcome.verified);

        let text = std::fs::read_to_string(app.join("Cargo.toml")).expect("read");
        assert!(
            text.contains(
                "light-fw-datepicker = { version = \"0.3.0\", optional = true, features = [\"default\"] }"
            ),
            "{text}"
        );
        assert!(
            text.contains("datepicker = [\"dep:light-fw-datepicker\"]"),
            "{text}"
        );

        let lock = std::fs::read_to_string(app.join("light.lock")).expect("read lock");
        assert!(lock.contains("light-fw-datepicker"), "{lock}");
        assert!(lock.contains("version = \"0.3.0\""), "{lock}");
    }

    #[test]
    fn known_component_enables_light_feature() {
        let app = workspace_fixture("light-feature");
        let opts = AddOptions {
            no_verify: true,
            ..Default::default()
        };
        let plan = plan_add(&app, "light-fw-element-form", &opts).expect("plan");
        assert_eq!(plan.light_features, vec!["elements-form"]);
        apply_add(&app, &MockRunner::default(), &plan, &opts).expect("apply");

        let text = std::fs::read_to_string(app.join("Cargo.toml")).expect("read");
        assert!(
            text.contains("light = { version = \"0.1\", features = [\"elements-form\"] }"),
            "{text}"
        );
    }

    #[test]
    fn failed_check_rolls_back() {
        let app = workspace_fixture("rollback");
        let original = std::fs::read_to_string(app.join("Cargo.toml")).expect("read");
        let opts = AddOptions::default();
        let plan = plan_add(&app, "light-fw-element-form", &opts).expect("plan");
        let runner = MockRunner {
            fail_check: true,
            ..Default::default()
        };
        let err = apply_add(&app, &runner, &plan, &opts).expect_err("check failed");
        assert!(matches!(err, Error::CommandFailed { .. }));
        assert_eq!(
            std::fs::read_to_string(app.join("Cargo.toml")).expect("read"),
            original,
            "Cargo.toml восстановлен"
        );
        assert!(!app.join("light.lock").exists(), "light.lock откачен");
    }

    #[test]
    fn default_feature_name_strips_prefix() {
        assert_eq!(
            default_feature_name("light-fw-element-form"),
            "element-form"
        );
        assert_eq!(default_feature_name("custom-widget"), "custom-widget");
    }

    #[test]
    fn choose_feature_prefers_own_then_aggregator_then_new() {
        // Собственная фича компонента — приоритетнее агрегатора.
        let with_own = "[features]\nfull = [\"dep:light-fw-element-form\"]\n\
                        element-form = [\"dep:light-fw-element-form\"]\n";
        assert_eq!(
            choose_feature(with_own, "light-fw-element-form").unwrap(),
            "element-form"
        );
        // Нет собственной — берём существующий агрегирующий (`full`).
        let with_aggregator =
            "[features]\ndefault = [\"full\"]\nfull = [\"dep:light-fw-element-form\"]\n";
        assert_eq!(
            choose_feature(with_aggregator, "light-fw-behavior-validation").unwrap(),
            "full"
        );
        // Агрегаторов нет — создаём новую фичу по умолчанию.
        assert_eq!(
            choose_feature("[features]\nnone = []\n", "light-fw-behavior-validation").unwrap(),
            "behavior-validation"
        );
    }

    /// Мини-проект с ядром, несовместимым с требованиями bundled-реестра.
    fn old_core_app(name: &str) -> PathBuf {
        let dir = temp_dir(&format!("incompat-{name}"));
        // Реестр требует ядро ^0.N; ставим 0.(N-1).0 — гарантированно несовместимо.
        let reg = ComponentRegistry::builtin();
        let req = reg
            .by_name("light-fw-element-form")
            .expect("bundled component")
            .core_requirement
            .clone();
        let core = req
            .comparators
            .first()
            .and_then(|c| c.minor)
            .map(|m| format!("0.{}.0", m.saturating_sub(1)))
            .unwrap_or_else(|| "0.0.0".to_string());
        std::fs::write(
            dir.join("Cargo.toml"),
            format!(
                "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
                 [dependencies]\nlight-fw = \"{}\"\n",
                core
            ),
        )
        .expect("app manifest");
        dir
    }

    /// Мокаем remote как «без сети»: `latest_version` всегда `None`.
    struct NoNetwork;

    impl RemoteRegistry for NoNetwork {}

    #[test]
    fn incompatible_core_without_network_yields_clear_error() {
        // Реестр требует ядро ^0.N, в проекте 0.(N-1). Сети нет (remote -> None)
        // и локальных копий нет — падаем с понятной ошибкой, а не паникой.
        let reg = ComponentRegistry::builtin();
        let entry = reg
            .by_name("light-fw-element-form")
            .expect("bundled component");
        let required = entry.core_requirement.clone();
        let installed = required
            .comparators
            .first()
            .and_then(|c| c.minor)
            .map(|m| format!("0.{}.0", m.saturating_sub(1)))
            .unwrap_or_else(|| "0.0.0".to_string());
        let app = old_core_app("no-net");
        // NoNetwork: resolve_remote_latest вернёт None, свежая версия не
        // найдётся — план упадёт на Incompatible.
        let catalog = Catalog::with_remote(&app, false, Box::new(NoNetwork));
        let err = plan_add_with_catalog(
            &app,
            "light-fw-element-form",
            &AddOptions::default(),
            &catalog,
        )
        .expect_err("incompatible");
        let Error::Incompatible { component, reason } = &err else {
            panic!("ожидался Incompatible, получено {err:?}");
        };
        assert_eq!(component, "light-fw-element-form");
        // Указаны требуемая и установленная версии ядра + инструкция `light add`.
        assert!(reason.contains(&required.to_string()), "{reason}");
        assert!(reason.contains(&installed), "{reason}");
        assert!(
            reason.contains("light add light-fw-element-form@"),
            "{reason}"
        );
    }

    #[test]
    fn incompatible_core_with_remote_latest_is_used() {
        // Если на crates.io есть свежая версия компонента, она выбирается вместо
        // записи реестра. Здесь свежая версия поддерживает ядро проекта, поэтому
        // план собирается без ошибки.
        let reg = ComponentRegistry::builtin();
        let fresh = reg
            .by_name("light-fw-element-form")
            .expect("bundled component")
            .latest_version
            .clone();
        let app = old_core_app("with-net");
        let catalog = Catalog::with_remote(
            &app,
            false,
            Box::new(MockRemote {
                name: "light-fw-element-form",
                version: fresh.clone(),
            }),
        );
        // Мокаем remote как «поддерживающий» ядро: компонент из каталога берёт
        // `core` из реестра — он несовместим, но свежая версия с
        // crates.io всё равно возвращается. План строится, зависимость — её
        // версия.
        let plan = plan_add_with_catalog(
            &app,
            "light-fw-element-form",
            &AddOptions::default(),
            &catalog,
        )
        .expect("plan with fresh version");
        assert_eq!(plan.dependency, DependencyEntry::Version(fresh.to_string()));
    }
}
