//! Логика команды `relay update`: обновление компонентов до актуальных версий.
//!
//! Версия локальных (workspace/vendor) компонентов определяется самим крейтом,
//! поэтому они помечаются как не требующие обновления. Для компонентов с
//! crates.io источником актуальной считается версия из встроенного реестра.
//! Сетевые запросы не выполняются — команда работает офлайн.

use std::path::Path;

use crate::error::{Error, Result};
use crate::manifest::{
    DepDetail, collect_dependency_details, read_cargo_toml, resolve_dependency_version,
};
use crate::process::CommandRunner;
use crate::registry::ComponentRegistry;
use crate::{backup, cargo as cargo_cmd, edit, lockfile::Lockfile};

/// Опции команды `relay update`.
#[derive(Debug, Clone, Default)]
pub struct UpdateOptions {
    /// Обновить только этот компонент.
    pub component: Option<String>,
    /// Не задавать вопросов.
    pub yes: bool,
    /// Только показать план.
    pub dry_run: bool,
    /// Не запускать `cargo check`.
    pub no_verify: bool,
}

/// Один компонент в плане обновления.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateEntry {
    /// Имя крейта.
    pub name: String,
    /// Текущая версия.
    pub from: Option<String>,
    /// Актуальная версия.
    pub to: String,
    /// Есть ли что обновлять.
    pub changed: bool,
    /// Пояснение (для локальных компонентов).
    pub note: Option<String>,
}

/// План обновления.
#[derive(Debug, Clone)]
pub struct UpdatePlan {
    /// Компоненты.
    pub entries: Vec<UpdateEntry>,
}

impl UpdatePlan {
    /// Сколько компонентов изменится.
    pub fn changed_count(&self) -> usize {
        self.entries.iter().filter(|e| e.changed).count()
    }

    /// Человекочитаемое описание.
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        if self.entries.is_empty() {
            out.push_str("Нет установленных компонентов.\n");
            return out;
        }
        out.push_str("Обновление компонентов:\n");
        for entry in &self.entries {
            let from = entry.from.as_deref().unwrap_or("?");
            if entry.changed {
                out.push_str(&format!("  ↑ {} {} → {}\n", entry.name, from, entry.to));
            } else if let Some(note) = &entry.note {
                out.push_str(&format!("  · {} {from} ({note})\n", entry.name));
            } else {
                out.push_str(&format!("  = {} {from} (актуален)\n", entry.name));
            }
        }
        out
    }
}

/// Строит план обновления.
pub fn plan_update(project_root: &Path, opts: &UpdateOptions) -> Result<UpdatePlan> {
    let cargo_path = project_root.join("Cargo.toml");
    if !cargo_path.is_file() {
        return Err(Error::Manifest(format!(
            "Cargo.toml не найден в {}",
            project_root.display()
        )));
    }
    let text = read_cargo_toml(&cargo_path)?;
    let installed = collect_dependency_details(&text, project_root)?;
    let registry = ComponentRegistry::builtin();

    let mut entries = Vec::new();
    for dep in &installed {
        if registry.by_name(&dep.name).is_none() {
            continue;
        }
        if let Some(filter) = &opts.component
            && &dep.name != filter
        {
            continue;
        }
        entries.push(plan_entry(project_root, dep, &registry)?);
    }

    if let Some(filter) = &opts.component
        && entries.is_empty()
    {
        return Err(Error::NotInstalled(filter.clone()));
    }

    entries.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(UpdatePlan { entries })
}

fn plan_entry(
    project_root: &Path,
    dep: &DepDetail,
    registry: &ComponentRegistry,
) -> Result<UpdateEntry> {
    let from = resolve_dependency_version(project_root, &dep.name)?;
    let local = dep.workspace || dep.path.is_some();
    let to = if local {
        from.clone().or_else(|| {
            registry
                .by_name(&dep.name)
                .map(|i| i.latest_version.clone())
        })
    } else {
        registry
            .by_name(&dep.name)
            .map(|i| i.latest_version.clone())
    };
    let to = to.ok_or_else(|| {
        Error::Manifest(format!(
            "не удалось определить актуальную версию {}",
            dep.name
        ))
    })?;

    let changed = !local && from.as_ref().is_none_or(|f| to > *f);
    Ok(UpdateEntry {
        name: dep.name.clone(),
        from: from.map(|v| v.to_string()),
        to: to.to_string(),
        changed,
        note: local.then(|| "версия определяется локальным крейтом".to_string()),
    })
}

/// Применяет план обновления.
pub fn apply_update(
    project_root: &Path,
    runner: &dyn CommandRunner,
    plan: &UpdatePlan,
    opts: &UpdateOptions,
) -> Result<()> {
    if opts.dry_run || plan.changed_count() == 0 {
        return Ok(());
    }
    let snapshot = backup::create(project_root)?;
    match do_update(project_root, runner, plan, opts) {
        Ok(()) => Ok(()),
        Err(error) => {
            backup::restore(&snapshot)?;
            Err(error)
        }
    }
}

fn do_update(
    project_root: &Path,
    runner: &dyn CommandRunner,
    plan: &UpdatePlan,
    opts: &UpdateOptions,
) -> Result<()> {
    let cargo_path = project_root.join("Cargo.toml");
    let text = read_cargo_toml(&cargo_path)?;
    let mut doc = edit::parse(&text)?;
    for entry in plan.entries.iter().filter(|e| e.changed) {
        edit::set_dependency_version(&mut doc, &entry.name, &entry.to)?;
    }
    std::fs::write(&cargo_path, doc.to_string())?;

    Lockfile::from_project(project_root)?.write(project_root)?;
    if !opts.no_verify {
        cargo_cmd::run_check(runner, project_root, false)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::CommandOutput;
    use std::path::PathBuf;

    struct MockRunner;

    impl CommandRunner for MockRunner {
        fn run(
            &self,
            _program: &str,
            _args: &[String],
            _cwd: Option<&Path>,
        ) -> Result<CommandOutput> {
            Ok(CommandOutput {
                status: 0,
                stdout: String::new(),
                stderr: String::new(),
            })
        }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("relay-cli-update-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    const OLD_MANIFEST: &str = "[package]\nname = \"p\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
         [workspace]\n\n[dependencies]\nrelay = \"0.1\"\nrelay-element-form = \"0.0.1\"\n";

    #[test]
    fn plans_outdated_component() {
        let dir = temp_dir("plan");
        std::fs::write(dir.join("Cargo.toml"), OLD_MANIFEST).expect("write");
        // relay не компонент? — он в реестре как ядро? нет: registry знает только компоненты.
        let plan = plan_update(&dir, &UpdateOptions::default()).expect("plan");
        assert_eq!(plan.changed_count(), 1);
        let entry = &plan.entries[0];
        assert_eq!(entry.name, "relay-element-form");
        assert_eq!(entry.from.as_deref(), Some("0.0.1"));
        assert_eq!(entry.to, "0.3.0");
        assert!(plan.to_text().contains("→ 0.3.0"));
    }

    #[test]
    fn applies_version_update() {
        let dir = temp_dir("apply");
        std::fs::write(dir.join("Cargo.toml"), OLD_MANIFEST).expect("write");
        let plan = plan_update(&dir, &UpdateOptions::default()).expect("plan");
        apply_update(&dir, &MockRunner, &plan, &UpdateOptions::default()).expect("apply");
        let text = std::fs::read_to_string(dir.join("Cargo.toml")).expect("read");
        assert!(text.contains("relay-element-form = \"0.3.0\""), "{text}");
        assert!(dir.join("relay.lock").exists());
    }

    #[test]
    fn up_to_date_is_noop() {
        let dir = temp_dir("uptodate");
        std::fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"p\"\nversion = \"0.1.0\"\n\n[workspace]\n\n[dependencies]\nrelay-element-form = \"0.3.0\"\n",
        )
        .expect("write");
        let plan = plan_update(&dir, &UpdateOptions::default()).expect("plan");
        assert_eq!(plan.changed_count(), 0);
        apply_update(&dir, &MockRunner, &plan, &UpdateOptions::default()).expect("apply");
        assert!(!dir.join("relay.lock").exists(), "ничего не менялось");
    }

    #[test]
    fn filter_unknown_component_is_error() {
        let dir = temp_dir("filter");
        std::fs::write(dir.join("Cargo.toml"), OLD_MANIFEST).expect("write");
        let opts = UpdateOptions {
            component: Some("relay-widget-datepicker".to_string()),
            ..Default::default()
        };
        let err = plan_update(&dir, &opts).expect_err("not installed");
        assert!(matches!(err, Error::NotInstalled(_)));
    }
}
