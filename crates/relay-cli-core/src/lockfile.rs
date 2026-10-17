//! Формат и управление `relay.lock`.
//!
//! `relay.lock` фиксирует установленные компоненты и их версии (по аналогии с
//! `Cargo.lock`), обеспечивая воспроизводимость сборки. Файл генерируется
//! автоматически командами `relay add`/`remove`/`update`/`lock` — вручную
//! редактировать его не нужно.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::manifest::{
    collect_dependency_details, find_workspace_root, read_workspace_dependencies,
    resolve_dependency_version,
};
use crate::registry::ComponentRegistry;
use crate::scan::scan_templates;

/// Имя lock-файла в корне проекта.
pub const LOCKFILE_NAME: &str = "relay.lock";

/// Заголовок, записываемый в начало файла.
const HEADER: &str = "# relay.lock — генерируется автоматически, не редактировать вручную.\n";

/// Содержимое `relay.lock`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lockfile {
    /// Версия формата.
    pub version: u32,
    /// Версия ядра, под которую зафиксированы компоненты.
    pub core: String,
    /// Зафиксированные компоненты.
    #[serde(rename = "component", default)]
    pub components: Vec<LockComponent>,
}

/// Один зафиксированный компонент.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockComponent {
    /// Имя крейта.
    pub name: String,
    /// Версия.
    pub version: String,
    /// Источник: `workspace` | `crates.io` | `vendor`.
    pub source: String,
    /// Путь к локальному компоненту (для `workspace`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Зависимости компонента.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<LockDependency>,
}

/// Зависимость компонента.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockDependency {
    /// Имя крейта.
    pub name: String,
    /// Требование/версия.
    pub version: String,
}

impl Lockfile {
    /// Путь к lock-файлу в проекте.
    pub fn path_in(project_root: &Path) -> PathBuf {
        project_root.join(LOCKFILE_NAME)
    }

    /// Читает lock-файл с диска.
    pub fn read(project_root: &Path) -> Result<Lockfile> {
        let path = Self::path_in(project_root);
        let text = std::fs::read_to_string(&path)?;
        Self::from_toml(&text)
    }

    /// Разбирает lock-файл из TOML-текста.
    pub fn from_toml(text: &str) -> Result<Lockfile> {
        toml_edit::de::from_str(text).map_err(|e| crate::Error::Lockfile(e.to_string()))
    }

    /// Сериализует lock-файл в TOML-текст (с заголовком-комментарием).
    pub fn to_toml(&self) -> Result<String> {
        let body = toml_edit::ser::to_string_pretty(self)?;
        Ok(format!("{HEADER}{body}"))
    }

    /// Записывает lock-файл на диск.
    pub fn write(&self, project_root: &Path) -> Result<()> {
        let path = Self::path_in(project_root);
        std::fs::write(path, self.to_toml()?)?;
        Ok(())
    }

    /// Строит lock-файл из текущего `Cargo.toml` проекта.
    pub fn from_project(project_root: &Path) -> Result<Lockfile> {
        let registry = ComponentRegistry::builtin();
        let core = resolve_core_version(project_root)?;
        let details = collect_project_components(project_root)?;

        let mut components = Vec::new();
        for dep in details {
            let info = registry.by_name(&dep.name);
            let version = resolve_dependency_version(project_root, &dep.name)?
                .or_else(|| info.map(|i| i.latest_version.clone()))
                .map(|v| v.to_string())
                .unwrap_or_else(|| "0.0.0".to_string());
            let source = if dep.workspace || dep.path.is_some() {
                "workspace"
            } else {
                "crates.io"
            };
            components.push(LockComponent {
                name: dep.name.clone(),
                version,
                source: source.to_string(),
                path: dep.path.as_ref().map(|p| relative_path(project_root, p)),
                dependencies: vec![LockDependency {
                    name: "relay-core".to_string(),
                    version: core.clone(),
                }],
            });
        }
        components.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(Lockfile {
            version: 1,
            core,
            components,
        })
    }

    /// Сравнивает set компонентов (имя+версия) с другим lock-файлом.
    pub fn same_components(&self, other: &Lockfile) -> bool {
        let key = |l: &Lockfile| {
            let mut v: Vec<(String, String)> = l
                .components
                .iter()
                .map(|c| (c.name.clone(), c.version.clone()))
                .collect();
            v.sort();
            v
        };
        key(self) == key(other)
    }
}

/// Разница между двумя `relay.lock`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LockDiff {
    /// Компоненты, появившиеся в новом lock.
    pub added: Vec<String>,
    /// Компоненты, исчезнувшие из нового lock.
    pub removed: Vec<String>,
    /// Компоненты с изменившейся версией.
    pub changed: Vec<String>,
}

impl LockDiff {
    /// Нет ли изменений.
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.changed.is_empty()
    }

    /// Человекочитаемое описание.
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        for name in &self.added {
            out.push_str(&format!("  + {name}\n"));
        }
        for name in &self.removed {
            out.push_str(&format!("  - {name}\n"));
        }
        for name in &self.changed {
            out.push_str(&format!("  ~ {name}\n"));
        }
        out
    }
}

/// Сравнивает два lock-файла по составу компонентов и их версиям.
pub fn diff(old: &Lockfile, new: &Lockfile) -> LockDiff {
    use std::collections::BTreeMap;

    let old_map: BTreeMap<&str, &str> = old
        .components
        .iter()
        .map(|c| (c.name.as_str(), c.version.as_str()))
        .collect();
    let new_map: BTreeMap<&str, &str> = new
        .components
        .iter()
        .map(|c| (c.name.as_str(), c.version.as_str()))
        .collect();

    let mut diff = LockDiff::default();
    for (name, version) in &new_map {
        match old_map.get(name) {
            None => diff.added.push((*name).to_string()),
            Some(old_version) if old_version != version => {
                diff.changed.push((*name).to_string());
            }
            Some(_) => {}
        }
    }
    for name in old_map.keys() {
        if !new_map.contains_key(name) {
            diff.removed.push((*name).to_string());
        }
    }
    diff
}

/// Актуален ли существующий `relay.lock` относительно `Cargo.toml`.
pub fn is_actual(project_root: &Path) -> Result<bool> {
    let path = Lockfile::path_in(project_root);
    if !path.is_file() {
        return Ok(false);
    }
    let current = Lockfile::read(project_root)?;
    let expected = Lockfile::from_project(project_root)?;
    Ok(current.same_components(&expected) && current.core == expected.core)
}
/// Читает установленные компоненты проекта (только известные реестру).
fn collect_project_components(project_root: &Path) -> Result<Vec<crate::manifest::DepDetail>> {
    let cargo = project_root.join("Cargo.toml");
    let text = std::fs::read_to_string(&cargo)?;
    let details = collect_dependency_details(&text, project_root)?;
    let registry = ComponentRegistry::builtin();
    Ok(details
        .into_iter()
        .filter(|d| registry.by_name(&d.name).is_some())
        .collect())
}

fn resolve_core_version(project_root: &Path) -> Result<String> {
    let version = resolve_dependency_version(project_root, "relay-core")?
        .or(resolve_dependency_version(project_root, "relay")?);
    Ok(version
        .map(|v| v.to_string())
        .unwrap_or_else(|| "0.0.0".to_string()))
}

fn relative_path(_base: &Path, path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Определяет источник компонента (workspace / crates.io), не читая сеть.
pub fn detect_source(project_root: &Path, name: &str) -> Result<Option<String>> {
    if let Some(root) = find_workspace_root(project_root) {
        let ws = read_workspace_dependencies(&root)?;
        if ws.iter().any(|d| d.name == name) {
            return Ok(Some("workspace".to_string()));
        }
    }
    Ok(None)
}

/// Полный анализ проекта (переиспользуется в `lock`), возвращает число шаблонов.
pub fn template_count(project_root: &Path) -> Result<usize> {
    Ok(scan_templates(project_root)?.files.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("relay-cli-lock-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn toml_round_trip() {
        let lock = Lockfile {
            version: 1,
            core: "0.1.0".to_string(),
            components: vec![LockComponent {
                name: "relay-element-form".to_string(),
                version: "0.1.0".to_string(),
                source: "workspace".to_string(),
                path: Some("crates/relay-element-form".to_string()),
                dependencies: vec![LockDependency {
                    name: "relay-core".to_string(),
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
    fn from_project_reads_components() {
        let dir = temp_dir("from-project");
        fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname=\"p\"\nversion=\"0.1.0\"\n\n[dependencies]\nrelay = \"0.1\"\nrelay-element-form = \"0.1\"\n",
        )
        .expect("write");
        let lock = Lockfile::from_project(&dir).expect("from_project");
        assert_eq!(lock.version, 1);
        assert_eq!(lock.components.len(), 1);
        assert_eq!(lock.components[0].name, "relay-element-form");
        assert_eq!(lock.core, "0.1.0");
    }

    #[test]
    fn is_actual_detects_drift() {
        let dir = temp_dir("actual");
        fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname=\"p\"\nversion=\"0.1.0\"\n\n[dependencies]\nrelay = \"0.1\"\nrelay-element-form = \"0.1\"\n",
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
            "[package]\nname=\"p\"\nversion=\"0.1.0\"\n\n[dependencies]\nrelay = \"0.1\"\n",
        )
        .expect("write");
        assert!(!is_actual(&dir).expect("is_actual"));
    }

    #[test]
    fn diff_reports_added_removed_changed() {
        let base = Lockfile {
            version: 1,
            core: "0.1.0".to_string(),
            components: vec![
                component("relay-element-form", "0.1.0"),
                component("relay-behavior-validation", "0.1.0"),
            ],
        };
        let updated = Lockfile {
            version: 1,
            core: "0.1.0".to_string(),
            components: vec![
                component("relay-element-form", "0.2.0"),
                component("relay-widget-datepicker", "0.3.0"),
            ],
        };
        let changes = diff(&base, &updated);
        assert_eq!(changes.changed, vec!["relay-element-form"]);
        assert_eq!(changes.added, vec!["relay-widget-datepicker"]);
        assert_eq!(changes.removed, vec!["relay-behavior-validation"]);
        assert!(!changes.is_empty());
        assert!(changes.to_text().contains("~ relay-element-form"));
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
