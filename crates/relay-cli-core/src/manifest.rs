use std::path::{Path, PathBuf};
use std::str::FromStr;

use serde::Deserialize;
use toml_edit::DocumentMut;

use crate::error::{Error, Result};

/// Манифест компонента Relay (`[package.metadata.relay]`).
#[derive(Debug, Clone, Deserialize)]
pub struct RelayManifest {
    /// Тип компонента.
    pub kind: ComponentKind,
    /// Человекочитаемое имя.
    #[serde(rename = "display-name")]
    pub display_name: String,
    /// Требование к версии ядра (semver-range).
    pub core: String,
    /// Что компонент предоставляет.
    #[serde(default)]
    pub provides: Provides,
    /// Что компонент требует.
    #[serde(default)]
    pub requires: Requires,
    /// Компоненты, несовместимые с этим.
    #[serde(default)]
    pub conflicts: Vec<String>,
    /// Метаданные реестра.
    #[serde(default)]
    pub registry: RegistryMeta,
}

/// Тип компонента (см. §3 модели компонентов).
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ComponentKind {
    Element,
    Behavior,
    Widget,
    Integration,
    Subsystem,
    Dev,
}

/// Что компонент предоставляет.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Provides {
    /// Новые теги.
    #[serde(default)]
    pub elements: Vec<String>,
    /// Новые атрибуты.
    #[serde(default)]
    pub attributes: Vec<String>,
    /// Новые директивы.
    #[serde(default)]
    pub directives: Vec<String>,
}

/// Что компонент требует.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Requires {
    /// Обязательные компоненты (имя + semver-range).
    #[serde(default)]
    pub components: Vec<String>,
}

/// Метаданные компонента для реестра.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct RegistryMeta {
    /// Официальный ли компонент.
    #[serde(default)]
    pub official: bool,
    /// Теги для поиска.
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Зависимость из `[dependencies]` потребительского проекта.
#[derive(Debug, Clone)]
pub struct Dependency {
    /// Имя крейта.
    pub name: String,
    /// Версия, если указана строкой или как `version = "..."`.
    pub version: Option<String>,
}

/// Разбирает секцию `[package.metadata.relay]` из текста `Cargo.toml`.
///
/// Возвращает `Ok(None)`, если секции нет (обычный крейт без компонента).
pub fn parse_manifest_from_text(toml_text: &str) -> Result<Option<RelayManifest>> {
    let manifest: CargoManifest =
        toml_edit::de::from_str(toml_text).map_err(|e| Error::Manifest(e.to_string()))?;
    let Some(relay) = manifest
        .package
        .and_then(|pkg| pkg.metadata)
        .and_then(|meta| meta.relay)
    else {
        return Ok(None);
    };
    semver::VersionReq::from_str(&relay.core)
        .map_err(|e| Error::Manifest(format!("invalid core range: {e}")))?;
    Ok(Some(relay))
}

#[derive(Debug, Deserialize)]
struct CargoManifest {
    #[serde(default)]
    package: Option<PackageSection>,
}

#[derive(Debug, Deserialize)]
struct PackageSection {
    #[serde(default)]
    metadata: Option<MetadataSection>,
}

#[derive(Debug, Deserialize)]
struct MetadataSection {
    #[serde(default)]
    relay: Option<RelayManifest>,
}

/// Читает `Cargo.toml` с диска.
pub fn read_cargo_toml(path: &Path) -> Result<String> {
    std::fs::read_to_string(path).map_err(Into::into)
}

/// Собирает зависимости из секции `[dependencies]`.
pub fn collect_dependencies(cargo_toml: &str) -> Result<Vec<Dependency>> {
    let doc: DocumentMut = cargo_toml
        .parse::<DocumentMut>()
        .map_err(|e| Error::Manifest(e.to_string()))?;
    let mut dependencies = Vec::new();
    if let Some(deps) = doc.get("dependencies").and_then(|d| d.as_table_like()) {
        for (name, item) in deps.iter() {
            let version = if let Some(s) = item.as_str() {
                Some(s.to_string())
            } else {
                item.as_table_like()
                    .and_then(|t| t.get("version"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            };
            dependencies.push(Dependency {
                name: name.to_string(),
                version,
            });
        }
    }
    dependencies.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(dependencies)
}

/// Подробности одной зависимости (`[dependencies]` или `[workspace.dependencies]`).
#[derive(Debug, Clone)]
pub struct DepDetail {
    /// Имя крейта.
    pub name: String,
    /// Строковое требование версии (`"0.1"`, `"^0.1"`, `">=0.1,<0.2"`).
    pub version: Option<String>,
    /// Путь (`path = "..."`), относительный к каталогу манифеста.
    pub path: Option<PathBuf>,
    /// Ссылка на workspace-зависимость (`workspace = true`).
    pub workspace: bool,
    /// Необязательная зависимость (`optional = true`).
    pub optional: bool,
}

fn details_from_table(table: &dyn toml_edit::TableLike, base_dir: &Path) -> Vec<DepDetail> {
    let mut out = Vec::new();
    for (name, item) in table.iter() {
        if let Some(version) = item.as_str() {
            out.push(DepDetail {
                name: name.to_string(),
                version: Some(version.to_string()),
                path: None,
                workspace: false,
                optional: false,
            });
            continue;
        }
        let Some(tbl) = item.as_table_like() else {
            continue;
        };
        let version = tbl
            .get("version")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let path = tbl
            .get("path")
            .and_then(|v| v.as_str())
            .map(|p| base_dir.join(p));
        let workspace = tbl
            .get("workspace")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let optional = tbl
            .get("optional")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        out.push(DepDetail {
            name: name.to_string(),
            version,
            path,
            workspace,
            optional,
        });
    }
    out
}

/// Собирает подробности из секции `[dependencies]` манифеста.
pub fn collect_dependency_details(cargo_toml: &str, base_dir: &Path) -> Result<Vec<DepDetail>> {
    let doc: DocumentMut = cargo_toml
        .parse::<DocumentMut>()
        .map_err(|e| Error::Manifest(e.to_string()))?;
    let mut out = Vec::new();
    if let Some(deps) = doc.get("dependencies").and_then(|d| d.as_table_like()) {
        out.extend(details_from_table(deps, base_dir));
    }
    Ok(out)
}

/// Читает `[workspace.dependencies]` корневого манифеста.
pub fn read_workspace_dependencies(workspace_root: &Path) -> Result<Vec<DepDetail>> {
    let cargo = workspace_root.join("Cargo.toml");
    let text = read_cargo_toml(&cargo)?;
    let doc: DocumentMut = text
        .parse::<DocumentMut>()
        .map_err(|e| Error::Manifest(e.to_string()))?;
    let mut out = Vec::new();
    if let Some(deps) = doc
        .get("workspace")
        .and_then(|w| w.as_table_like())
        .and_then(|w| w.get("dependencies"))
        .and_then(|d| d.as_table_like())
    {
        out.extend(details_from_table(deps, workspace_root));
    }
    Ok(out)
}

/// Читает имя пакета из текста `Cargo.toml`.
pub fn read_package_name_from_text(cargo_toml: &str) -> Result<Option<String>> {
    let doc: DocumentMut = cargo_toml
        .parse::<DocumentMut>()
        .map_err(|e| Error::Manifest(e.to_string()))?;
    Ok(doc
        .get("package")
        .and_then(|p| p.as_table_like())
        .and_then(|p| p.get("name"))
        .and_then(|n| n.as_str())
        .map(str::to_string))
}

/// Проверяет наличие секции `[package]` в манифесте.
pub fn has_package_section(cargo_toml: &str) -> Result<bool> {
    let doc: DocumentMut = cargo_toml
        .parse::<DocumentMut>()
        .map_err(|e| Error::Manifest(e.to_string()))?;
    Ok(doc.get("package").is_some())
}

/// Делает путь абсолютным относительно текущего каталога, не обращаясь к ФС.
///
/// Нужно перед подъёмом вверх по дереву каталогов: у относительного пути вида
/// `.` нет родительского компонента, поэтому [`Path::pop`] не может подняться
/// выше (`crates/relay-demo` → `crates` → корень теряется).
pub fn absolute(path: &Path) -> PathBuf {
    if path.is_absolute() {
        return path.to_path_buf();
    }
    match std::env::current_dir() {
        Ok(cwd) => cwd.join(path),
        Err(_) => path.to_path_buf(),
    }
}

/// Находит корень workspace, поднимаясь от `start` вверх по каталогам.
///
/// Возвращает первый каталог с `Cargo.toml`, содержащим секцию `[workspace]`.
pub fn find_workspace_root(start: &Path) -> Option<PathBuf> {
    let start = absolute(start);
    let mut dir = if start.is_file() {
        start.parent().map(Path::to_path_buf)?
    } else {
        start
    };
    loop {
        let cargo = dir.join("Cargo.toml");
        if cargo.is_file()
            && let Ok(text) = std::fs::read_to_string(&cargo)
            && let Ok(doc) = text.parse::<DocumentMut>()
            && doc.get("workspace").is_some()
        {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// Читает версию пакета из манифеста, разрешая `version.workspace = true`.
pub fn read_package_version(crate_dir: &Path) -> Result<Option<semver::Version>> {
    let cargo = crate_dir.join("Cargo.toml");
    let text = read_cargo_toml(&cargo)?;
    let doc: DocumentMut = text
        .parse::<DocumentMut>()
        .map_err(|e| Error::Manifest(e.to_string()))?;
    let Some(pkg) = doc.get("package").and_then(|p| p.as_table_like()) else {
        return Ok(None);
    };
    match pkg.get("version") {
        Some(v) if v.as_str().is_some() => {
            Ok(semver::Version::parse(v.as_str().expect("checked")).ok())
        }
        Some(v) if v.as_table_like().is_some() => {
            let is_workspace = v
                .as_table_like()
                .and_then(|t| t.get("workspace"))
                .and_then(|w| w.as_bool())
                .unwrap_or(false);
            if is_workspace && let Some(root) = find_workspace_root(crate_dir) {
                return read_workspace_package_version(&root);
            }
            Ok(None)
        }
        _ => Ok(None),
    }
}

fn read_workspace_package_version(workspace_root: &Path) -> Result<Option<semver::Version>> {
    let cargo = workspace_root.join("Cargo.toml");
    let text = read_cargo_toml(&cargo)?;
    let doc: DocumentMut = text
        .parse::<DocumentMut>()
        .map_err(|e| Error::Manifest(e.to_string()))?;
    let version = doc
        .get("workspace")
        .and_then(|w| w.as_table_like())
        .and_then(|w| w.get("package"))
        .and_then(|p| p.as_table_like())
        .and_then(|p| p.get("version"))
        .and_then(|v| v.as_str())
        .and_then(|s| semver::Version::parse(s).ok());
    Ok(version)
}

/// Минимальная версия, удовлетворяющая требованию (нижняя граница первого компаратора).
pub fn min_version(req: &semver::VersionReq) -> Option<semver::Version> {
    let c = req.comparators.first()?;
    Some(semver::Version::new(
        c.major,
        c.minor.unwrap_or(0),
        c.patch.unwrap_or(0),
    ))
}

/// Разрешает версию зависимости `name` в проекте `project_dir`.
///
/// Учитывает `version = "..."`, `path = "..."` и `workspace = true`.
pub fn resolve_dependency_version(
    project_dir: &Path,
    name: &str,
) -> Result<Option<semver::Version>> {
    let cargo = project_dir.join("Cargo.toml");
    let text = read_cargo_toml(&cargo)?;
    let details = collect_dependency_details(&text, project_dir)?;
    let Some(dep) = details.into_iter().find(|d| d.name == name) else {
        return Ok(None);
    };
    if dep.workspace
        && let Some(root) = find_workspace_root(project_dir)
    {
        let ws = read_workspace_dependencies(&root)?;
        if let Some(ws_dep) = ws.into_iter().find(|d| d.name == name) {
            return resolve_detail_version(&ws_dep);
        }
    }
    resolve_detail_version(&dep)
}

fn resolve_detail_version(dep: &DepDetail) -> Result<Option<semver::Version>> {
    if let Some(path) = &dep.path {
        return read_package_version(path);
    }
    if let Some(req) = &dep.version {
        let req = semver::VersionReq::parse(req)?;
        return Ok(min_version(&req));
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_component_manifest() {
        let toml = r#"
            [package]
            name = "relay-widget-datepicker"
            version = "0.3.1"

            [package.metadata.relay]
            kind = "widget"
            display-name = "Date Picker"
            core = "^0.3"
            conflicts = ["relay-widget-old-datepicker"]

            [package.metadata.relay.provides]
            elements = ["datepicker"]

            [package.metadata.relay.requires]
            components = ["relay-element-form ^0.3"]

            [package.metadata.relay.registry]
            official = false
            tags = ["form", "date"]
        "#;
        let manifest = parse_manifest_from_text(toml)
            .expect("parse")
            .expect("manifest present");
        assert_eq!(manifest.kind, ComponentKind::Widget);
        assert_eq!(manifest.display_name, "Date Picker");
        assert_eq!(manifest.provides.elements, vec!["datepicker"]);
        assert_eq!(
            manifest.requires.components,
            vec!["relay-element-form ^0.3"]
        );
        assert_eq!(manifest.conflicts, vec!["relay-widget-old-datepicker"]);
        assert_eq!(manifest.registry.tags, vec!["form", "date"]);
    }

    #[test]
    fn returns_none_without_metadata() {
        let toml = "[package]\nname = \"plain\"\nversion = \"0.1.0\"\n";
        assert!(parse_manifest_from_text(toml).expect("parse").is_none());
    }

    #[test]
    fn rejects_bad_core_range() {
        let toml = r#"
            [package.metadata.relay]
            kind = "element"
            display-name = "Broken"
            core = "not-a-range"
        "#;
        assert!(parse_manifest_from_text(toml).is_err());
    }

    #[test]
    fn finds_workspace_root_from_relative_path() {
        let root = find_workspace_root(Path::new(".")).expect("workspace root from '.'");
        assert!(root.is_absolute(), "корень должен быть абсолютным");
        assert!(root.join("Cargo.toml").is_file());
        assert_eq!(
            find_workspace_root(Path::new("src")).expect("workspace root from 'src'"),
            root
        );
    }

    #[test]
    fn collects_dependencies_variants() {
        let toml = r#"
            [dependencies]
            relay = { version = "0.3", features = ["elements-basic"] }
            relay-element-form = "0.3"
            serde = { version = "1", features = ["derive"] }
            local = { path = "../local" }
        "#;
        let deps = collect_dependencies(toml).expect("collect");
        assert_eq!(deps.len(), 4);
        let by_name = |n: &str| deps.iter().find(|d| d.name == n).expect("dep");
        assert_eq!(by_name("relay").version.as_deref(), Some("0.3"));
        assert_eq!(
            by_name("relay-element-form").version.as_deref(),
            Some("0.3")
        );
        assert_eq!(by_name("serde").version.as_deref(), Some("1"));
        assert!(by_name("local").version.is_none());
    }
}
