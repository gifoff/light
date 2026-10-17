//! Логика команды `relay add`: план и применение добавления компонента.
//!
//! Разделение на [`plan_add`] (только чтение) и [`apply_add`] (мутации) даёт
//! `--dry-run` «бесплатно» и делает проверки тестируемыми без записи на диск.
//! Перед мутациями создаётся резервная копия; при любой ошибке (включая провал
//! `cargo check`) проект восстанавливается.

use std::path::Path;

use crate::error::{Error, Result};
use crate::fetch::{Catalog, DependencyEntry, ResolvedComponent};
use crate::manifest::{
    collect_dependency_details, has_package_section, read_cargo_toml, resolve_dependency_version,
};
use crate::process::CommandRunner;
use crate::{backup, cargo as cargo_cmd, edit, lockfile::Lockfile};

/// Опции команды `relay add`.
#[derive(Debug, Clone, Default)]
pub struct AddOptions {
    /// Явная версия (`--version`).
    pub version: Option<String>,
    /// Не задавать вопросов (`--yes`).
    pub yes: bool,
    /// Только показать план (`--dry-run`).
    pub dry_run: bool,
    /// Без сетевого доступа (`--offline`).
    pub offline: bool,
    /// Фича, к которой привязать зависимость (`--feature`).
    pub feature: Option<String>,
    /// Не запускать `cargo check` (`--no-verify`).
    pub no_verify: bool,
}

/// Компонент, который нужно установить дополнительно (из `requires`).
#[derive(Debug, Clone)]
pub struct RequiredComponent {
    /// Имя крейта.
    pub name: String,
    /// Как записать зависимость.
    pub dependency: DependencyEntry,
}

/// Подготовленный план добавления компонента.
#[derive(Debug, Clone)]
pub struct AddPlan {
    /// Разрешённый компонент.
    pub component: ResolvedComponent,
    /// Как записать зависимость.
    pub dependency: DependencyEntry,
    /// Фича, к которой привязывается зависимость.
    pub feature: String,
    /// Будет ли фича создана (иначе — уже существует).
    pub creates_feature: bool,
    /// Дополнительные компоненты из `requires`.
    pub requires: Vec<RequiredComponent>,
}

impl AddPlan {
    /// Человекочитаемое описание плана.
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("План добавления {}\n", self.component.name));
        out.push_str(&format!(
            "  источник:    {} ({})\n",
            self.component.source.as_str(),
            match &self.component.path {
                Some(p) => p.display().to_string(),
                None => "crates.io".to_string(),
            }
        ));
        out.push_str(&format!("  версия:      {}\n", self.component.version));
        out.push_str(&format!(
            "  фича:        {}{}\n",
            self.feature,
            if self.creates_feature {
                " (будет создана)"
            } else {
                ""
            }
        ));
        out.push_str(&format!(
            "  зависимость: {}\n",
            dependency_line(&self.component.name, &self.dependency)
        ));
        for req in &self.requires {
            out.push_str(&format!(
                "  также нужно: {} — {}\n",
                req.name,
                dependency_line(&req.name, &req.dependency)
            ));
        }
        out
    }
}

/// Результат применения плана.
#[derive(Debug, Clone)]
pub struct AddOutcome {
    /// Установленный компонент.
    pub component: ResolvedComponent,
    /// Фича, к которой привязана зависимость.
    pub feature: String,
    /// Дополнительно установленные компоненты.
    pub added_components: Vec<String>,
    /// Запускалась ли проверка сборки.
    pub verified: bool,
}

/// Строит план добавления компонента `name` в проект `project_root`.
pub fn plan_add(project_root: &Path, name: &str, opts: &AddOptions) -> Result<AddPlan> {
    let cargo_path = project_root.join("Cargo.toml");
    if !cargo_path.is_file() {
        return Err(Error::Manifest(format!(
            "Cargo.toml не найден в {}",
            project_root.display()
        )));
    }
    let text = read_cargo_toml(&cargo_path)?;
    if !has_package_section(&text)? {
        return Err(Error::Manifest(
            "Cargo.toml не содержит секции [package]: укажите каталог приложения".to_string(),
        ));
    }

    let installed = collect_dependency_details(&text, project_root)?;
    let installed_names: Vec<&str> = installed.iter().map(|d| d.name.as_str()).collect();

    if installed_names.contains(&name) {
        return Err(Error::AlreadyInstalled(name.to_string()));
    }

    let catalog = Catalog::discover(project_root, opts.offline);
    let mut component = catalog.resolve(name)?;
    if let Some(version) = &opts.version {
        component.version = semver::Version::parse(version)?;
    }

    let core = resolve_dependency_version(project_root, "relay-core")?
        .or(resolve_dependency_version(project_root, "relay")?);
    if let Some(core) = core
        && !component.core.matches(&core)
    {
        return Err(Error::Incompatible {
            component: name.to_string(),
            reason: format!("требуется relay-core {}, в проекте {core}", component.core),
        });
    }

    for conflict in &component.conflicts {
        if installed_names.iter().any(|n| n == conflict) {
            return Err(Error::Conflict(format!(
                "{name} несовместим с установленным {conflict}"
            )));
        }
    }

    let default_feature = default_feature_name(name);
    let feature = opts.feature.clone().unwrap_or(default_feature);
    let creates_feature = !edit::has_feature(&text, &feature)?;

    let dependency = catalog.dependency_entry(&component)?;

    let mut requires = Vec::new();
    for requirement in &component.requires {
        let (dep_name, _) = split_requirement(requirement);
        if is_core_name(dep_name) || installed_names.contains(&dep_name) {
            continue;
        }
        if requires
            .iter()
            .any(|r: &RequiredComponent| r.name == dep_name)
        {
            continue;
        }
        let resolved = catalog.resolve(dep_name)?;
        requires.push(RequiredComponent {
            name: dep_name.to_string(),
            dependency: catalog.dependency_entry(&resolved)?,
        });
    }

    Ok(AddPlan {
        component,
        dependency,
        feature,
        creates_feature,
        requires,
    })
}

/// Применяет план: мутирует проект и (при необходимости) проверяет сборку.
pub fn apply_add(
    project_root: &Path,
    runner: &dyn CommandRunner,
    plan: &AddPlan,
    opts: &AddOptions,
) -> Result<AddOutcome> {
    if opts.dry_run {
        return Ok(AddOutcome {
            component: plan.component.clone(),
            feature: plan.feature.clone(),
            added_components: plan.requires.iter().map(|r| r.name.clone()).collect(),
            verified: false,
        });
    }

    let snapshot = backup::create(project_root)?;
    match do_add(project_root, runner, plan, opts) {
        Ok(outcome) => Ok(outcome),
        Err(error) => {
            backup::restore(&snapshot)?;
            Err(error)
        }
    }
}

fn do_add(
    project_root: &Path,
    runner: &dyn CommandRunner,
    plan: &AddPlan,
    opts: &AddOptions,
) -> Result<AddOutcome> {
    let cargo_path = project_root.join("Cargo.toml");

    add_dependency(project_root, runner, &plan.component.name, &plan.dependency)?;
    let mut added_components = Vec::new();
    for req in &plan.requires {
        add_dependency(project_root, runner, &req.name, &req.dependency)?;
        added_components.push(req.name.clone());
    }

    let text = read_cargo_toml(&cargo_path)?;
    let mut doc = edit::parse(&text)?;
    edit::add_dep_to_feature(&mut doc, &plan.feature, &plan.component.name)?;
    for req in &plan.requires {
        edit::add_dep_to_feature(&mut doc, &plan.feature, &req.name)?;
    }
    std::fs::write(&cargo_path, doc.to_string())?;

    Lockfile::from_project(project_root)?.write(project_root)?;

    let verified = !opts.no_verify;
    if verified {
        cargo_cmd::run_check(runner, project_root, false)?;
    }

    Ok(AddOutcome {
        component: plan.component.clone(),
        feature: plan.feature.clone(),
        added_components,
        verified,
    })
}

fn add_dependency(
    project_root: &Path,
    runner: &dyn CommandRunner,
    name: &str,
    dependency: &DependencyEntry,
) -> Result<()> {
    let cargo_path = project_root.join("Cargo.toml");
    match dependency {
        DependencyEntry::Workspace => {
            let text = read_cargo_toml(&cargo_path)?;
            let mut doc = edit::parse(&text)?;
            edit::set_workspace_optional(&mut doc, name)?;
            std::fs::write(&cargo_path, doc.to_string())?;
        }
        DependencyEntry::Path(path) => {
            cargo_cmd::run_add(runner, project_root, name, None, Some(path), true)?;
        }
        DependencyEntry::Version(version) => {
            cargo_cmd::run_add(runner, project_root, name, Some(version), None, true)?;
        }
    }
    Ok(())
}

fn dependency_line(name: &str, dependency: &DependencyEntry) -> String {
    match dependency {
        DependencyEntry::Workspace => format!("{name} = {{ workspace = true, optional = true }}"),
        DependencyEntry::Path(path) => {
            format!(
                "{name} = {{ path = \"{}\", optional = true }}",
                path.display()
            )
        }
        DependencyEntry::Version(version) => {
            format!("{name} = {{ version = \"{version}\", optional = true }}")
        }
    }
}

/// Имя фичи по умолчанию: `relay-element-form` → `element-form`.
pub fn default_feature_name(name: &str) -> String {
    name.strip_prefix("relay-").unwrap_or(name).to_string()
}

/// Разбирает `"relay-element-form ^0.3"` → `("relay-element-form", Some("^0.3"))`.
fn split_requirement(requirement: &str) -> (&str, Option<&str>) {
    match requirement.split_once(char::is_whitespace) {
        Some((name, rest)) => (name, Some(rest.trim())),
        None => (requirement, None),
    }
}

fn is_core_name(name: &str) -> bool {
    matches!(name, "relay" | "relay-core")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::CommandOutput;
    use std::path::PathBuf;
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
        ) -> Result<CommandOutput> {
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
        let dir = std::env::temp_dir().join(format!("relay-cli-add-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    /// Создаёт мини-workspace с компонентом `relay-element-form`.
    fn workspace_fixture(name: &str) -> PathBuf {
        let root = temp_dir(name);
        std::fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"app\", \"comp\"]\nresolver = \"2\"\n\n\
             [workspace.dependencies]\nrelay-element-form = { path = \"comp\", version = \"0.1.0\" }\n",
        )
        .expect("root manifest");
        let app = root.join("app");
        std::fs::create_dir_all(&app).expect("app");
        std::fs::write(
            app.join("Cargo.toml"),
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
             [dependencies]\nrelay = \"0.1\"\n",
        )
        .expect("app manifest");
        let comp = root.join("comp");
        std::fs::create_dir_all(comp.join("src")).expect("comp src");
        std::fs::write(comp.join("src/lib.rs"), "").expect("comp lib");
        std::fs::write(
            comp.join("Cargo.toml"),
            "[package]\nname = \"relay-element-form\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
             [package.metadata.relay]\nkind = \"element\"\ndisplay-name = \"Form\"\ncore = \"^0.1\"\n\n\
             [package.metadata.relay.provides]\nelements = [\"checkbox\"]\n",
        )
        .expect("comp manifest");
        app
    }

    #[test]
    fn plans_workspace_component() {
        let app = workspace_fixture("plan");
        let plan = plan_add(&app, "relay-element-form", &AddOptions::default()).expect("plan");
        assert_eq!(plan.dependency, DependencyEntry::Workspace);
        assert_eq!(plan.feature, "element-form");
        assert!(plan.creates_feature);
        assert!(plan.to_text().contains("relay-element-form"));
    }

    #[test]
    fn already_installed_is_error() {
        let app = workspace_fixture("installed");
        let opts = AddOptions {
            no_verify: true,
            ..Default::default()
        };
        let plan = plan_add(&app, "relay-element-form", &opts).expect("plan");
        apply_add(&app, &MockRunner::default(), &plan, &opts).expect("apply");
        let err = plan_add(&app, "relay-element-form", &opts).expect_err("already installed");
        assert!(matches!(err, Error::AlreadyInstalled(_)));
    }

    #[test]
    fn dry_run_does_not_touch_disk() {
        let app = workspace_fixture("dry-run");
        let original = std::fs::read_to_string(app.join("Cargo.toml")).expect("read");
        let opts = AddOptions {
            dry_run: true,
            ..Default::default()
        };
        let plan = plan_add(&app, "relay-element-form", &opts).expect("plan");
        apply_add(&app, &MockRunner::default(), &plan, &opts).expect("apply");
        assert_eq!(
            std::fs::read_to_string(app.join("Cargo.toml")).expect("read"),
            original
        );
        assert!(!app.join("relay.lock").exists());
    }

    #[test]
    fn applies_workspace_dependency_and_feature() {
        let app = workspace_fixture("apply");
        let opts = AddOptions::default();
        let plan = plan_add(&app, "relay-element-form", &opts).expect("plan");
        let runner = MockRunner::default();
        let outcome = apply_add(&app, &runner, &plan, &opts).expect("apply");
        assert!(outcome.verified);

        let text = std::fs::read_to_string(app.join("Cargo.toml")).expect("read");
        assert!(text.contains("relay-element-form = { workspace = true, optional = true }"));
        assert!(text.contains("[features]"));
        assert!(text.contains("element-form = [\"dep:relay-element-form\"]"));
        assert!(app.join("relay.lock").exists());

        let calls = runner.calls.lock().expect("lock").clone();
        assert!(calls.iter().any(|c| c == "cargo check --quiet"));
    }

    #[test]
    fn failed_check_rolls_back() {
        let app = workspace_fixture("rollback");
        let original = std::fs::read_to_string(app.join("Cargo.toml")).expect("read");
        let opts = AddOptions::default();
        let plan = plan_add(&app, "relay-element-form", &opts).expect("plan");
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
        assert!(!app.join("relay.lock").exists(), "relay.lock откачен");
    }

    #[test]
    fn default_feature_name_strips_prefix() {
        assert_eq!(default_feature_name("relay-element-form"), "element-form");
        assert_eq!(default_feature_name("custom-widget"), "custom-widget");
    }
}
