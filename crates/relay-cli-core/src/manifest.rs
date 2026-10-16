use std::path::Path;
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
