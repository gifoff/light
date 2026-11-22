#[cfg(test)]
#[allow(clippy::module_inception)]
mod tests {
    use crate::error::Error;
    use crate::init::config::*;
    use crate::init::generate::*;
    use crate::init::run::*;
    use crate::init::types::*;
    use crate::lockfile::{LOCKFILE_NAME, Lockfile};
    use crate::process::{CommandOutput, CommandRunner};
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn scratch(name: &str) -> PathBuf {
        // CWD-относительный каталог: `temp_dir()` может быть недоступен в
        // sandboxed окружении (PermissionDenied при записи), а текущий каталог —
        // всегда. Ключ из pid + нано-времени + счётчика уникален на хосте.
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let pid = std::process::id();
        let dir = std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(format!("light-fw-cli-init-{name}-{pid}-{nanos}-{unique}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    struct StubRunner {
        found: bool,
    }

    impl CommandRunner for StubRunner {
        fn run(
            &self,
            program: &str,
            _args: &[String],
            _cwd: Option<&Path>,
        ) -> std::result::Result<CommandOutput, crate::error::Error> {
            if self.found {
                Ok(CommandOutput {
                    status: 0,
                    stdout: "cargo 1.99.0".to_string(),
                    stderr: String::new(),
                })
            } else {
                Err(Error::CommandNotFound(program.to_string()))
            }
        }
    }

    #[test]
    fn rejects_invalid_names() {
        assert!(validate_name("my-app").is_ok());
        assert!(validate_name("app_2").is_ok());
        assert!(validate_name("").is_err());
        assert!(validate_name("2app").is_err());
        assert!(validate_name("my app").is_err());
        assert!(validate_name("app!").is_err());
    }

    #[test]
    fn plan_default_creates_all_files() {
        let root = scratch("plan-default");
        let dir = root.join("cool-app");
        let options = InitOptions {
            name: "cool-app".to_string(),
            path: Some(dir.clone()),
            minimal: false,
            components: Vec::new(),
            template: None,
        };
        let plan = plan(&options).expect("plan");
        let names: Vec<String> = plan
            .files
            .iter()
            .map(|(path, _)| path.to_string_lossy().replace('\\', "/"))
            .collect();
        assert!(names.contains(&"Cargo.toml".to_string()));
        assert!(names.contains(&"src/main.rs".to_string()));
        assert!(names.contains(&"templates/app.html".to_string()));
        assert!(names.contains(&".gitignore".to_string()));
        assert!(names.contains(&".cargo/config.toml".to_string()));
        assert!(names.contains(&"light.config.json".to_string()));
        assert!(names.contains(&"AGENT.md".to_string()));
        // main.rs использует `#[template]` (HTML-шаблон).
        let main = plan
            .files
            .iter()
            .find(|(path, _)| path.to_string_lossy().replace('\\', "/") == "src/main.rs")
            .map(|(_, c)| c.as_str())
            .expect("main.rs in plan");
        assert!(main.contains("#[template"), "{main}");
        assert!(plan.write_lockfile);
    }

    #[test]
    fn agent_md_is_always_generated_with_core_cheatsheet() {
        // В каждом проекте (default и minimal) есть `AGENT.md` с ключевыми
        // строками шпаргалки: не читать исходники, `<text>` — поле ввода,
        // команды `light devtools`, ссылка на guide.md.
        for options in [
            InitOptions {
                name: "with-agent".to_string(),
                path: None,
                minimal: false,
                components: Vec::new(),
                template: None,
            },
            InitOptions {
                name: "minimal-agent".to_string(),
                path: None,
                minimal: true,
                components: Vec::new(),
                template: Some(InitTemplate::Counter),
            },
        ] {
            let plan = plan(&options).expect("plan");
            let text = plan
                .files
                .iter()
                .find(|(path, _)| path.to_string_lossy().replace('\\', "/") == "AGENT.md")
                .map(|(_, c)| c.as_str())
                .unwrap_or_else(|| {
                    panic!("AGENT.md отсутствует в плане (minimal={})", options.minimal)
                });
            assert!(text.contains("НЕ ЧИТАЙ ИСХОДНИКИ"), "{text}");
            assert!(text.contains("поле ввода"), "{text}");
            assert!(text.contains("light devtools"), "{text}");
            assert!(text.contains("docs/guide.md"), "{text}");
        }
    }

    #[test]
    fn plan_minimal_omits_gitignore_and_lockfile() {
        let root = scratch("plan-minimal");
        let options = InitOptions {
            name: "tiny".to_string(),
            path: Some(root.join("tiny")),
            minimal: true,
            components: Vec::new(),
            template: None,
        };
        let plan = plan(&options).expect("plan");
        let names: Vec<String> = plan
            .files
            .iter()
            .map(|(path, _)| path.to_string_lossy().replace('\\', "/"))
            .collect();
        assert!(!names.contains(&".gitignore".to_string()));
        assert!(!plan.write_lockfile);
    }

    #[test]
    fn plan_rejects_non_empty_directory() {
        let root = scratch("plan-nonempty");
        let target = root.join("busy");
        std::fs::create_dir_all(&target).expect("dir");
        std::fs::write(target.join("file.txt"), "x").expect("file");
        let options = InitOptions {
            name: "busy".to_string(),
            path: Some(target),
            minimal: false,
            components: Vec::new(),
            template: None,
        };
        let error = plan(&options).expect_err("должна быть ошибка");
        assert!(error.to_string().contains("не пуст"), "{error}");
    }

    #[test]
    fn cargo_toml_pins_light_version() {
        let text = cargo_toml("demo", &[]);
        assert!(text.contains("name = \"demo\""));
        let mv = minor_version();
        // Обёртка переименована: новый проект зависит от `light-fw`, а не `light`.
        assert!(text.contains(&format!("light-fw = \"{mv}\"")));
        assert!(!text.contains(&format!("light = \"{mv}\"")), "{text}");
        // Фича `dev-mode` всегда присутствует (алиас `light-fw/dev-mode`).
        assert!(text.contains("[features]"), "{text}");
        assert!(
            text.contains("dev-mode = [\"light-fw/dev-mode\"]"),
            "{text}"
        );
        // ТЗ 0.16.1: в самом конце — пустой `[workspace]`.
        assert!(text.trim_end().ends_with("[workspace]"), "{text}");
    }

    #[test]
    fn cargo_toml_adds_form_dependency_and_feature() {
        let text = cargo_toml("demo", &[InitComponent::Form]);
        let mv = minor_version();
        // Мета-крейт — `light-fw` (не `light`).
        assert!(
            text.contains(&format!(
                "light-fw = {{ version = \"{mv}\", features = [\"elements-form\"] }}"
            )),
            "{text}"
        );
        assert!(
            text.contains(&format!("light-fw-element-form = \"{mv}\"")),
            "{text}"
        );
        assert!(!text.contains("light-fw-behavior-validation"), "{text}");
    }

    #[test]
    fn cargo_toml_adds_both_dependencies_and_deduplicates() {
        let text = cargo_toml(
            "demo",
            &[
                InitComponent::Validation,
                InitComponent::Form,
                InitComponent::Form,
            ],
        );
        let mv = minor_version();
        // Мета-крейт — `light-fw` (не `light`).
        assert!(
            text.contains(&format!(
                "light-fw = {{ version = \"{mv}\", features = [\"elements-form\", \"validation\"] }}"
            )),
            "{text}"
        );
        assert_eq!(text.matches("light-fw-element-form =").count(), 1, "{text}");
        assert_eq!(
            text.matches("light-fw-behavior-validation =").count(),
            1,
            "{text}"
        );
    }

    #[test]
    fn main_rs_uses_template_macro() {
        let text = main_rs(&[]);
        assert!(text.contains("#[template"), "{text}");
        assert!(text.contains("templates/app.html"), "{text}");
        assert!(text.contains("#[proxy]"), "{text}");
        assert!(text.contains("App::new()"), "{text}");
        assert!(text.contains("run_with_config"), "{text}");
        assert!(!text.contains("include_str!"));
        assert!(!text.contains("Пример формы"));
    }

    #[test]
    fn main_rs_includes_form_example_only_for_form_component() {
        let text = main_rs(&[InitComponent::Form]);
        assert!(text.contains("Пример формы"), "{text}");
        let validation_only = main_rs(&[InitComponent::Validation]);
        assert!(
            !validation_only.contains("Пример формы"),
            "{validation_only}"
        );
    }

    #[test]
    fn parses_supported_templates() {
        assert_eq!(
            InitTemplate::parse("counter").expect("counter"),
            InitTemplate::Counter
        );
        assert_eq!(
            InitTemplate::parse("todo").expect("todo"),
            InitTemplate::Todo
        );
        assert_eq!(
            InitTemplate::parse("calculator").expect("calculator"),
            InitTemplate::Calculator
        );
        let error = InitTemplate::parse("unknown").expect_err("unknown template");
        assert!(error.to_string().contains("counter"), "{error}");
    }

    #[test]
    fn template_plan_includes_main_and_html() {
        let root = scratch("plan-template");
        let options = InitOptions {
            name: "todo-app".to_string(),
            path: Some(root.join("todo-app")),
            minimal: true,
            components: Vec::new(),
            template: Some(InitTemplate::Todo),
        };
        let plan = plan(&options).expect("plan");
        let by_path = |name: &str| {
            plan.files
                .iter()
                .find(|(path, _)| path.to_string_lossy().replace('\\', "/") == name)
                .map(|(_, content)| content.as_str())
        };
        assert!(by_path("src/main.rs").expect("main").contains("TodoApp"));
        assert!(
            by_path("templates/todo.html")
                .expect("todo template")
                .contains("Light Todo")
        );
        // Cargo.toml + main.rs + шаблон + `.cargo/config.toml` + `light.config.json`
        // + `AGENT.md` (шпаргалка для AI-агентов) + `docs/guide.md` + `docs/cli.md`.
        assert_eq!(
            plan.files.len(),
            8,
            "Cargo.toml + main.rs + template + cargo config + light config \
             + AGENT.md + docs/guide.md + docs/cli.md"
        );
        assert!(
            by_path(".cargo/config.toml")
                .expect("cargo config")
                .contains("dev = \"run --features light-fw/dev-mode\"")
        );
        assert!(
            by_path("light.config.json")
                .expect("light config")
                .contains("\"port\": 9229")
        );
    }

    #[test]
    fn require_cargo_reports_missing_tool() {
        let error = require_cargo(&StubRunner { found: false }).expect_err("нет cargo");
        assert!(error.to_string().contains("cargo"), "{error}");
        assert!(require_cargo(&StubRunner { found: true }).is_ok());
    }

    #[test]
    fn apply_creates_project_and_lockfile() {
        let root = scratch("apply");
        let dir = root.join("my-app");
        let options = InitOptions {
            name: "my-app".to_string(),
            path: Some(dir.clone()),
            minimal: false,
            components: Vec::new(),
            template: None,
        };
        let report = apply(&StubRunner { found: true }, &options).expect("apply");
        assert!(report.lockfile);
        assert!(dir.join("Cargo.toml").is_file());
        assert!(dir.join("src").join("main.rs").is_file());
        assert!(dir.join(".gitignore").is_file());
        assert!(dir.join(LOCKFILE_NAME).is_file());

        let lock = Lockfile::read(&dir).expect("read lock");
        // Ядро в lockfile — нижняя граница минорного диапазона: `cargo_toml`
        // прописывает `light-fw = "0.20"` (minor_version()), а `resolve_core_version`
        // берёт `min_version("0.20")` = `0.20.0`. Патч-версию не проверяем — она
        // зависит от релиза и не входит в контракт init.
        let expected_minor = minor_version();
        let expected_core = format!("{}.0", expected_minor);
        assert_eq!(
            lock.core, expected_core,
            "ядро в lockfile — min_version(minor_version())"
        );
    }

    #[test]
    fn apply_minimal_skips_lockfile() {
        let root = scratch("apply-minimal");
        let dir = root.join("tiny");
        let options = InitOptions {
            name: "tiny".to_string(),
            path: Some(dir.clone()),
            minimal: true,
            components: Vec::new(),
            template: None,
        };
        let report = apply(&StubRunner { found: true }, &options).expect("apply");
        assert!(!report.lockfile);
        assert!(dir.join("Cargo.toml").is_file());
        assert!(!dir.join(".gitignore").exists());
        assert!(!dir.join(LOCKFILE_NAME).exists());
    }

    #[test]
    fn apply_fails_without_cargo_before_writing() {
        let root = scratch("apply-nocargo");
        let dir = root.join("nope");
        let options = InitOptions {
            name: "nope".to_string(),
            path: Some(dir.clone()),
            minimal: false,
            components: Vec::new(),
            template: None,
        };
        assert!(apply(&StubRunner { found: false }, &options).is_err());
        assert!(!dir.exists(), "каталог не должен создаваться");
    }

    /// Регрессия 0.17.1, задача 1: `light init` **никогда** не записывает
    /// `default-features = false` в `Cargo.toml` нового проекта. Зависимость
    /// `light-fw` должна остаться с дефолтными фичами (строка или inline-таблица
    /// с `version`/`features`, но без `default-features = false`).
    #[test]
    fn cargo_toml_never_disables_default_features() {
        // Без компонентов: `light-fw = "0.17"`.
        let bare = cargo_toml("bare-app", &[]);
        assert!(
            !bare.contains("default-features"),
            "bare project must not mention default-features:\n{bare}"
        );

        // С компонентами: `light-fw = { version, features = [...] }` — тоже без
        // `default-features = false`.
        let with_components = cargo_toml(
            "form-app",
            &[InitComponent::Form, InitComponent::Validation],
        );
        assert!(
            !with_components.contains("default-features"),
            "project with --with components must not disable default features:\n{with_components}"
        );
        // Фичи компонентов при этом на месте.
        assert!(
            with_components.contains("\"elements-form\""),
            "missing elements-form feature"
        );
        assert!(
            with_components.contains("\"validation\""),
            "missing validation feature"
        );
    }
}
