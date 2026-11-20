use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::manifest::resolve_dependency_version;
use crate::registry::ComponentRegistry;

use super::project::{collect_project_components, relative_path, resolve_core_version};

/// Имя lock-файла в корне проекта.
pub const LOCKFILE_NAME: &str = "light.lock";

/// Заголовок, записываемый в начало файла.
const HEADER: &str = "# light.lock — генерируется автоматически, не редактировать вручную.\n";

/// Содержимое `light.lock`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lockfile {
    /// Версия формата.
    pub version: u32,
    /// Версия ядра, под которую зафиксированы компоненты.
    pub core: String,
    /// Зафиксированные компоненты.
    #[serde(rename = "component", default)]
    pub components: Vec<LockComponent>,
    /// Приоритет по тегам элемента при конфликте: тег → победитель (имя крейта).
    ///
    /// Этап 2 Plugin API: когда два установленных компонента предоставляют
    /// один и тот же элемент (`provides.elements`), `light add --force`
    /// фиксирует, какой из них «выигрывает». Секция `[plugins]` в `light.lock`,
    /// например `datepicker = "light-widget-datepicker"`. Старые файлы без
    /// секции разбираются как пустой набор ([`serde::default`](default)).
    #[serde(default)]
    pub plugins: BTreeMap<String, String>,
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
    ///
    /// Секция `[plugins]` (приоритеты по тегам) **переносится** из
    /// существующего `light.lock`, если он есть: она не выводится из
    /// `Cargo.toml`, а живёт только в lock-файле и пишется `light add --force`.
    /// Если `light.lock` ещё не существует — приоритетов нет (пустой набор).
    pub fn from_project(project_root: &Path) -> Result<Lockfile> {
        let registry = ComponentRegistry::builtin();
        let core = resolve_core_version(project_root)?;
        let details = collect_project_components(project_root)?;
        let existing_plugins = Self::read(project_root)
            .map(|lf| lf.plugins)
            .unwrap_or_default();

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
                    name: "light-fw-core".to_string(),
                    version: core.clone(),
                }],
            });
        }
        components.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(Lockfile {
            version: 1,
            core,
            components,
            plugins: existing_plugins,
        })
    }

    /// Фиксирует приоритет по тегу: победителем (владельцем тега) становится
    /// `crate_name`. Этап 2 Plugin API: вызывается при `light add --force`,
    /// когда новый компонент «вытесняет» уже установленный владельца общего
    /// элемента. Повторный вызов перезаписывает прежнего победителя.
    pub fn set_plugin_priority(&mut self, tag: &str, crate_name: &str) {
        self.plugins.insert(tag.to_string(), crate_name.to_string());
    }

    /// Победитель по тегу (если приоритет был зафиксирован).
    pub fn plugin_priority(&self, tag: &str) -> Option<&str> {
        self.plugins.get(tag).map(String::as_str)
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

/// Разница между двумя `light.lock`.
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
