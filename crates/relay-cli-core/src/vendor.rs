//! Логика команды `relay vendor`: копирование исходников компонентов в `vendor/`.
//!
//! В `vendor/` складываются локальные (workspace/vendor) компоненты, чтобы
//! проект можно было собрать без доступа к сети и без соседних репозиториев.
//! Компоненты, доступные только на crates.io, пропускаются с предупреждением.

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::fetch::Catalog;
use crate::manifest::{collect_dependency_details, read_cargo_toml};
use crate::registry::ComponentRegistry;

/// Каталог по умолчанию.
pub const DEFAULT_VENDOR_DIR: &str = "vendor";

/// Каталоги, которые не копируются.
const EXCLUDED_DIRS: [&str; 2] = ["target", ".git"];

/// Опции команды `relay vendor`.
#[derive(Debug, Clone)]
pub struct VendorOptions {
    /// Куда складывать исходники.
    pub output: PathBuf,
    /// Только показать план.
    pub dry_run: bool,
    /// Режим без сети.
    pub offline: bool,
}

impl Default for VendorOptions {
    fn default() -> Self {
        Self {
            output: PathBuf::from(DEFAULT_VENDOR_DIR),
            dry_run: false,
            offline: false,
        }
    }
}

/// Один копируемый компонент.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VendorEntry {
    /// Имя крейта.
    pub name: String,
    /// Откуда копируем.
    pub from: PathBuf,
    /// Куда копируем.
    pub to: PathBuf,
}

/// План вендоринга.
#[derive(Debug, Clone)]
pub struct VendorPlan {
    /// Копируемые компоненты.
    pub entries: Vec<VendorEntry>,
    /// Компоненты, которые не удалось вендорить (только crates.io).
    pub skipped: Vec<String>,
}

impl VendorPlan {
    /// Человекочитаемое описание.
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        if self.entries.is_empty() {
            out.push_str("Нечего вендорить (нет локальных компонентов).\n");
        } else {
            out.push_str("Вендоринг компонентов:\n");
            for entry in &self.entries {
                out.push_str(&format!("  {} → {}\n", entry.name, entry.to.display()));
            }
        }
        for name in &self.skipped {
            out.push_str(&format!(
                "  ⚠ {name}: нет локальных исходников (будет взят с crates.io)\n"
            ));
        }
        out
    }
}

/// Строит план вендоринга.
pub fn plan_vendor(project_root: &Path, opts: &VendorOptions) -> Result<VendorPlan> {
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
    let catalog = Catalog::discover(project_root, opts.offline);
    let output = resolve_output(project_root, &opts.output);

    let mut entries = Vec::new();
    let mut skipped = Vec::new();
    for dep in &installed {
        if registry.by_name(&dep.name).is_none() {
            continue;
        }
        match catalog.resolve(&dep.name) {
            Ok(component) => match component.path {
                Some(from) => entries.push(VendorEntry {
                    name: dep.name.clone(),
                    to: output.join(&dep.name),
                    from,
                }),
                None => skipped.push(dep.name.clone()),
            },
            Err(_) => skipped.push(dep.name.clone()),
        }
    }

    entries.sort_by(|a, b| a.name.cmp(&b.name));
    skipped.sort();
    Ok(VendorPlan { entries, skipped })
}

/// Применяет план вендоринга.
pub fn apply_vendor(plan: &VendorPlan, dry_run: bool) -> Result<Vec<String>> {
    if dry_run {
        return Ok(Vec::new());
    }
    let mut vendored = Vec::new();
    for entry in &plan.entries {
        if entry.to == entry.from || entry.to.starts_with(&entry.from) {
            continue;
        }
        copy_dir_filtered(&entry.from, &entry.to)?;
        vendored.push(entry.name.clone());
    }
    Ok(vendored)
}

fn resolve_output(project_root: &Path, output: &Path) -> PathBuf {
    if output.is_absolute() {
        output.to_path_buf()
    } else {
        project_root.join(output)
    }
}

fn copy_dir_filtered(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if entry.file_type()?.is_dir() {
            if EXCLUDED_DIRS.contains(&name_str.as_ref()) {
                continue;
            }
            copy_dir_filtered(&entry.path(), &dst.join(&name))?;
        } else {
            std::fs::copy(entry.path(), dst.join(&name))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("relay-cli-vendor-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn installed_workspace(name: &str) -> PathBuf {
        let root = temp_dir(name);
        std::fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"app\", \"comp\"]\nresolver = \"2\"\n\n\
             [workspace.dependencies]\nrelay-element-form = { path = \"comp\", version = \"0.1.0\" }\n",
        )
        .expect("root manifest");
        let app = root.join("app");
        std::fs::create_dir_all(app.join("src")).expect("app src");
        std::fs::write(app.join("src/lib.rs"), "").expect("app lib");
        std::fs::write(
            app.join("Cargo.toml"),
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
             [dependencies]\nrelay-element-form = { workspace = true, optional = true }\n",
        )
        .expect("app manifest");
        let comp = root.join("comp");
        std::fs::create_dir_all(comp.join("src")).expect("comp src");
        std::fs::write(comp.join("src/lib.rs"), "// form").expect("comp lib");
        std::fs::write(
            comp.join("Cargo.toml"),
            "[package]\nname = \"relay-element-form\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
             [package.metadata.relay]\nkind = \"element\"\ndisplay-name = \"Form\"\ncore = \"^0.1\"\n\n\
             [package.metadata.relay.provides]\nelements = [\"checkbox\"]\n",
        )
        .expect("comp manifest");
        // Мусор, который не должен копироваться.
        std::fs::create_dir_all(comp.join("target/debug")).expect("target");
        std::fs::write(comp.join("target/debug/junk"), "junk").expect("junk");
        app
    }

    #[test]
    fn plans_workspace_components() {
        let app = installed_workspace("plan");
        let plan = plan_vendor(&app, &VendorOptions::default()).expect("plan");
        assert_eq!(plan.entries.len(), 1);
        assert_eq!(plan.entries[0].name, "relay-element-form");
        assert!(plan.to_text().contains("relay-element-form"));
    }

    #[test]
    fn copies_sources_excluding_target() {
        let app = installed_workspace("apply");
        let plan = plan_vendor(&app, &VendorOptions::default()).expect("plan");
        let vendored = apply_vendor(&plan, false).expect("apply");
        assert_eq!(vendored, vec!["relay-element-form"]);

        let dst = app.join("vendor/relay-element-form");
        assert!(dst.join("Cargo.toml").is_file());
        assert!(dst.join("src/lib.rs").is_file());
        assert!(!dst.join("target").exists(), "target не копируется");
    }

    #[test]
    fn dry_run_does_not_copy() {
        let app = installed_workspace("dry-run");
        let plan = plan_vendor(&app, &VendorOptions::default()).expect("plan");
        apply_vendor(&plan, true).expect("dry-run");
        assert!(!app.join("vendor").exists());
    }
}
