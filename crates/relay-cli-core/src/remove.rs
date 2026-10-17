//! Логика команды `relay remove`: план и применение удаления компонента.
//!
//! Аналогично [`crate::add`], разделено на [`plan_remove`] и [`apply_remove`].
//! Перед мутацией создаётся резервная копия; при ошибке проект откатывается.

use std::path::Path;

use crate::error::{Error, Result};
use crate::fetch::Catalog;
use crate::manifest::{
    collect_dependency_details, has_package_section, read_cargo_toml, resolve_dependency_version,
};
use crate::process::CommandRunner;
use crate::registry::ComponentRegistry;
use crate::{backup, cargo as cargo_cmd, edit, lockfile::Lockfile};

/// Опции команды `relay remove`.
#[derive(Debug, Clone, Default)]
pub struct RemoveOptions {
    /// Не задавать вопросов (`--yes`).
    pub yes: bool,
    /// Только показать план (`--dry-run`).
    pub dry_run: bool,
    /// Без сетевого доступа (`--offline`).
    pub offline: bool,
}

/// План удаления компонента.
#[derive(Debug, Clone)]
pub struct RemovePlan {
    /// Имя крейта.
    pub name: String,
    /// Установленная версия (если определена).
    pub version: Option<String>,
    /// Фичи, из которых будет убран `dep:`-токен.
    pub features: Vec<String>,
    /// Установленные компоненты, зависящие от удаляемого.
    pub dependents: Vec<String>,
}

impl RemovePlan {
    /// Человекочитаемое описание плана.
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("План удаления {}\n", self.name));
        out.push_str(&format!(
            "  версия: {}\n",
            self.version.as_deref().unwrap_or("?")
        ));
        if self.features.is_empty() {
            out.push_str("  фичи:   (нет ссылок dep:)\n");
        } else {
            out.push_str(&format!("  фичи:   {}\n", self.features.join(", ")));
        }
        for dep in &self.dependents {
            out.push_str(&format!(
                "  предупреждение: от {dep} зависит этот компонент\n"
            ));
        }
        out
    }
}

/// Строит план удаления компонента `name`.
pub fn plan_remove(project_root: &Path, name: &str) -> Result<RemovePlan> {
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
    if !installed.iter().any(|d| d.name == name) {
        return Err(Error::NotInstalled(name.to_string()));
    }

    let version = resolve_dependency_version(project_root, name)?.map(|v| v.to_string());
    let features = edit::features_containing(&text, name)?;

    Ok(RemovePlan {
        name: name.to_string(),
        version,
        features,
        dependents: Vec::new(),
    })
}

/// Строит план удаления, определяя зависимые компоненты (нужен сетевой доступ
/// для удалённых компонентов, если не задан `--offline`).
pub fn plan_remove_with_dependents(
    project_root: &Path,
    name: &str,
    offline: bool,
) -> Result<RemovePlan> {
    let mut plan = plan_remove(project_root, name)?;
    let text = read_cargo_toml(&project_root.join("Cargo.toml"))?;
    let installed = collect_dependency_details(&text, project_root)?;
    let catalog = Catalog::discover(project_root, offline);
    let registry = ComponentRegistry::builtin();

    for dep in &installed {
        if dep.name == name || registry.by_name(&dep.name).is_none() {
            continue;
        }
        if let Ok(resolved) = catalog.resolve(&dep.name)
            && resolved
                .requires
                .iter()
                .any(|r| requirement_name(r) == name)
        {
            plan.dependents.push(dep.name.clone());
        }
    }
    plan.dependents.sort();
    Ok(plan)
}

/// Применяет план удаления: мутирует проект и проверяет сборку.
pub fn apply_remove(
    project_root: &Path,
    runner: &dyn CommandRunner,
    plan: &RemovePlan,
    opts: &RemoveOptions,
) -> Result<()> {
    if opts.dry_run {
        return Ok(());
    }
    let snapshot = backup::create(project_root)?;
    match do_remove(project_root, runner, plan) {
        Ok(()) => Ok(()),
        Err(error) => {
            backup::restore(&snapshot)?;
            Err(error)
        }
    }
}

fn do_remove(project_root: &Path, runner: &dyn CommandRunner, plan: &RemovePlan) -> Result<()> {
    let cargo_path = project_root.join("Cargo.toml");
    let text = read_cargo_toml(&cargo_path)?;
    let dependency = collect_dependency_details(&text, project_root)?
        .into_iter()
        .find(|d| d.name == plan.name)
        .ok_or_else(|| Error::NotInstalled(plan.name.clone()))?;

    if dependency.workspace {
        let mut doc = edit::parse(&text)?;
        edit::remove_dependency(&mut doc, &plan.name)?;
        std::fs::write(&cargo_path, doc.to_string())?;
    } else {
        cargo_cmd::run_remove(runner, project_root, &plan.name)?;
        let text = read_cargo_toml(&cargo_path)?;
        let mut doc = edit::parse(&text)?;
        edit::remove_dependency(&mut doc, &plan.name)?;
        std::fs::write(&cargo_path, doc.to_string())?;
    }

    Lockfile::from_project(project_root)?.write(project_root)?;
    cargo_cmd::run_check(runner, project_root, false)?;
    Ok(())
}

fn requirement_name(requirement: &str) -> &str {
    requirement
        .split_once(char::is_whitespace)
        .map(|(name, _)| name)
        .unwrap_or(requirement)
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
            Ok(CommandOutput {
                status: 0,
                stdout: String::new(),
                stderr: String::new(),
            })
        }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("relay-cli-remove-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    /// Мини-workspace с уже установленным компонентом и фичей.
    fn installed_fixture(name: &str) -> PathBuf {
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
             [dependencies]\nrelay = \"0.1\"\nrelay-element-form = { workspace = true, optional = true }\n\n\
             [features]\ndefault = [\"full\"]\nfull = [\"dep:relay-element-form\"]\nelement-form = [\"dep:relay-element-form\"]\n",
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
    fn plans_removal_with_features() {
        let app = installed_fixture("plan");
        let plan = plan_remove(&app, "relay-element-form").expect("plan");
        assert_eq!(plan.version.as_deref(), Some("0.1.0"));
        assert_eq!(plan.features, vec!["element-form", "full"]);
        assert!(plan.to_text().contains("relay-element-form"));
    }

    #[test]
    fn not_installed_is_error() {
        let app = installed_fixture("missing");
        let err = plan_remove(&app, "relay-widget-datepicker").expect_err("not installed");
        assert!(matches!(err, Error::NotInstalled(_)));
    }

    #[test]
    fn removes_dependency_and_lockfile() {
        let app = installed_fixture("apply");
        let plan = plan_remove(&app, "relay-element-form").expect("plan");
        let runner = MockRunner::default();
        apply_remove(&app, &runner, &plan, &RemoveOptions::default()).expect("apply");

        let text = std::fs::read_to_string(app.join("Cargo.toml")).expect("read");
        assert!(!text.contains("relay-element-form"));
        assert!(text.contains("full = [\"relay/x\"]") || text.contains("default = [\"full\"]"));
        assert!(app.join("relay.lock").exists());
        let calls = runner.calls.lock().expect("lock").clone();
        assert!(calls.iter().any(|c| c == "cargo check --quiet"));
    }

    #[test]
    fn dry_run_does_not_touch_disk() {
        let app = installed_fixture("dry-run");
        let original = std::fs::read_to_string(app.join("Cargo.toml")).expect("read");
        let plan = plan_remove(&app, "relay-element-form").expect("plan");
        let opts = RemoveOptions {
            dry_run: true,
            ..Default::default()
        };
        apply_remove(&app, &MockRunner::default(), &plan, &opts).expect("apply");
        assert_eq!(
            std::fs::read_to_string(app.join("Cargo.toml")).expect("read"),
            original
        );
    }
}
