//! Каталог компонентов: где искать метаданные крейта-компонента.
//!
//! Разрешение идёт в порядке: **workspace** → **vendor** → **crates.io**.
//! Реальные сетевые запросы спрятаны за фичей `network`; без неё crates.io
//! заменяется встроенным реестром (`registry.toml`), поэтому ядро CLI
//! собирается и тестируется без сети.

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::manifest::{
    DepDetail, Provides, absolute, find_workspace_root, parse_manifest_from_text, read_cargo_toml,
    read_package_name_from_text, read_package_version, read_workspace_dependencies,
};
use crate::registry::ComponentRegistry;

/// Источник, из которого разрешён компонент.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    /// Локальный член workspace.
    Workspace,
    /// Локальная копия из `vendor/`.
    Vendor,
    /// Опубликованный крейт на crates.io.
    CratesIo,
}

impl SourceKind {
    /// Строковое имя для `light.lock`.
    pub fn as_str(self) -> &'static str {
        match self {
            SourceKind::Workspace => "workspace",
            SourceKind::Vendor => "vendor",
            SourceKind::CratesIo => "crates.io",
        }
    }
}

/// Разрешённый компонент с метаданными.
#[derive(Debug, Clone)]
pub struct ResolvedComponent {
    /// Имя крейта.
    pub name: String,
    /// Версия компонента.
    pub version: semver::Version,
    /// Требование к версии ядра.
    pub core: semver::VersionReq,
    /// Фичи мета-крейта `light`, которые включает компонент.
    pub light_features: Vec<String>,
    /// Что предоставляет компонент.
    pub provides: Provides,
    /// Обязательные компоненты (`"name ^ver"`).
    pub requires: Vec<String>,
    /// Несовместимые компоненты.
    pub conflicts: Vec<String>,
    /// Источник.
    pub source: SourceKind,
    /// Путь к локальному компоненту (для workspace/vendor).
    pub path: Option<PathBuf>,
}

impl ResolvedComponent {
    fn from_registry(info: &crate::registry::ComponentInfo) -> Self {
        Self {
            name: info.name.clone(),
            version: info.latest_version.clone(),
            core: info.core_requirement.clone(),
            light_features: info.light_features.clone(),
            provides: info.provides.clone(),
            requires: Vec::new(),
            conflicts: Vec::new(),
            source: SourceKind::CratesIo,
            path: None,
        }
    }
}

/// Источник последней версии крейта на crates.io.
///
/// Трейт позволяет подменять сетевой запрос в тестах.
pub trait RemoteRegistry: Send + Sync {
    /// Возвращает последнюю стабильную версию или `None`, если крейт не найден.
    fn latest_version(&self, name: &str) -> Result<Option<semver::Version>>;
}

/// Реальный клиент crates.io API (работает только с фичей `network`).
#[derive(Debug, Default, Clone, Copy)]
pub struct CratesIoRegistry;

impl RemoteRegistry for CratesIoRegistry {
    fn latest_version(&self, name: &str) -> Result<Option<semver::Version>> {
        #[cfg(feature = "network")]
        {
            fetch_crates_io(name)
        }
        #[cfg(not(feature = "network"))]
        {
            let _ = name;
            Ok(None)
        }
    }
}

/// Каталог разрешения компонентов для проекта.
pub struct Catalog {
    project_root: PathBuf,
    workspace_root: Option<PathBuf>,
    vendor_dir: Option<PathBuf>,
    offline: bool,
    registry: ComponentRegistry,
    remote: Box<dyn RemoteRegistry>,
}

impl Catalog {
    /// Создаёт каталог для проекта, автоматически находя workspace и `vendor/`.
    pub fn discover(project_root: &Path, offline: bool) -> Self {
        Self::with_remote(project_root, offline, Box::new(CratesIoRegistry))
    }

    /// Создаёт каталог с подменяемым источником crates.io (для тестов и
    /// встраивания).
    pub fn with_remote(
        project_root: &Path,
        offline: bool,
        remote: Box<dyn RemoteRegistry>,
    ) -> Self {
        let project_root = absolute(project_root);
        let vendor = project_root.join("vendor");
        Self {
            workspace_root: find_workspace_root(&project_root),
            vendor_dir: vendor.is_dir().then_some(vendor),
            project_root,
            offline,
            registry: ComponentRegistry::builtin(),
            remote,
        }
    }

    /// Встроенный реестр.
    pub fn registry(&self) -> &ComponentRegistry {
        &self.registry
    }

    /// Разрешает компонент по имени.
    pub fn resolve(&self, name: &str) -> Result<ResolvedComponent> {
        if let Some(component) = self.resolve_local(name)? {
            return Ok(component);
        }
        self.resolve_remote(name)
    }

    /// Разрешает компонент только среди локальных (workspace/vendor), без сети.
    pub fn resolve_local(&self, name: &str) -> Result<Option<ResolvedComponent>> {
        if let Some(component) = self.resolve_workspace(name)? {
            return Ok(Some(component));
        }
        self.resolve_vendor(name)
    }

    fn resolve_workspace(&self, name: &str) -> Result<Option<ResolvedComponent>> {
        let Some(root) = &self.workspace_root else {
            return Ok(None);
        };
        let deps = read_workspace_dependencies(root)?;
        let dir = match deps.into_iter().find(|d| d.name == name) {
            Some(dep) => match dep.path {
                Some(p) => p,
                None => root.join("crates").join(name),
            },
            // Даже если зависимости нет в `[workspace.dependencies]`, компонент
            // может быть членом workspace по соглашению `crates/<name>`.
            None => {
                let candidate = root.join("crates").join(name);
                if candidate.join("Cargo.toml").is_file() {
                    candidate
                } else {
                    return Ok(None);
                }
            }
        };
        let cargo = dir.join("Cargo.toml");
        if !cargo.is_file() {
            return Ok(None);
        }
        // Убеждаемся, что это тот самый компонент (а не одноимённый каталог).
        let text = read_cargo_toml(&cargo)?;
        if read_package_name_from_text(&text)?.as_deref() != Some(name) {
            return Ok(None);
        }
        if parse_manifest_from_text(&text)?.is_none() {
            return Ok(None);
        }
        Ok(Some(self.component_from_dir(
            name,
            &dir,
            SourceKind::Workspace,
        )?))
    }

    fn resolve_vendor(&self, name: &str) -> Result<Option<ResolvedComponent>> {
        let Some(vendor) = &self.vendor_dir else {
            return Ok(None);
        };
        let dir = vendor.join(name);
        if !dir.join("Cargo.toml").is_file() {
            return Ok(None);
        }
        Ok(Some(self.component_from_dir(
            name,
            &dir,
            SourceKind::Vendor,
        )?))
    }

    fn component_from_dir(
        &self,
        name: &str,
        dir: &Path,
        source: SourceKind,
    ) -> Result<ResolvedComponent> {
        let text = read_cargo_toml(&dir.join("Cargo.toml"))?;
        let manifest = parse_manifest_from_text(&text)?
            .ok_or_else(|| Error::Manifest(format!("в {name} нет [package.metadata.light]")))?;
        let version = read_package_version(dir)?
            .or_else(|| {
                self.registry
                    .by_name(name)
                    .map(|i| i.latest_version.clone())
            })
            .ok_or_else(|| Error::Manifest(format!("не удалось определить версию {name}")))?;
        Ok(ResolvedComponent {
            name: name.to_string(),
            version,
            core: manifest.core.parse()?,
            light_features: self
                .registry
                .by_name(name)
                .map(|info| info.light_features.clone())
                .unwrap_or_default(),
            provides: manifest.provides,
            requires: manifest.requires.components,
            conflicts: manifest.conflicts,
            source,
            path: Some(dir.to_path_buf()),
        })
    }

    fn resolve_remote(&self, name: &str) -> Result<ResolvedComponent> {
        let info = self.registry.by_name(name);
        if info.is_none() && !is_component_crate_name(name) {
            return Err(Error::NotComponent(name.to_string()));
        }
        if self.offline {
            return Err(Error::Offline(name.to_string()));
        }
        match self.remote.latest_version(name)? {
            Some(version) => {
                let (core, light_features, provides) = match info {
                    Some(i) => (
                        i.core_requirement.clone(),
                        i.light_features.clone(),
                        i.provides.clone(),
                    ),
                    None => (semver::VersionReq::STAR, Vec::new(), Provides::default()),
                };
                Ok(ResolvedComponent {
                    name: name.to_string(),
                    version,
                    core,
                    light_features,
                    provides,
                    requires: Vec::new(),
                    conflicts: Vec::new(),
                    source: SourceKind::CratesIo,
                    path: None,
                })
            }
            None => match info {
                Some(i) => Ok(ResolvedComponent::from_registry(i)),
                None => Err(Error::ComponentNotFound(name.to_string())),
            },
        }
    }

    /// Описание зависимости, как её следует записать в `Cargo.toml` проекта.
    pub fn dependency_entry(&self, component: &ResolvedComponent) -> Result<DependencyEntry> {
        // Если workspace определяет зависимость — используем `workspace = true`.
        if matches!(component.source, SourceKind::Workspace)
            && let Some(root) = &self.workspace_root
        {
            let deps = read_workspace_dependencies(root)?;
            if deps.iter().any(|d| d.name == component.name) {
                return Ok(DependencyEntry::Workspace);
            }
        }
        if let Some(path) = &component.path {
            return Ok(DependencyEntry::Path(relative_to(&self.project_root, path)));
        }
        Ok(DependencyEntry::Version(component.version.to_string()))
    }

    /// Возвращает `DepDetail`-совместимое описание (для проверок), если задано в workspace.
    pub fn workspace_detail(&self, name: &str) -> Result<Option<DepDetail>> {
        let Some(root) = &self.workspace_root else {
            return Ok(None);
        };
        Ok(read_workspace_dependencies(root)?
            .into_iter()
            .find(|d| d.name == name))
    }

    /// Корень проекта.
    pub fn project_root(&self) -> &Path {
        &self.project_root
    }

    /// Корень workspace (если найден).
    pub fn workspace_root(&self) -> Option<&Path> {
        self.workspace_root.as_deref()
    }

    /// Режим `--offline`.
    pub fn is_offline(&self) -> bool {
        self.offline
    }
}

/// Имя внешнего компонента Light: `light-fw-*`, кроме ядра.
pub fn is_component_crate_name(name: &str) -> bool {
    name.starts_with("light-fw-") && name != "light-fw-core"
}

/// Вычисляет путь `target` относительно каталога `base`.
///
/// Оба пути должны быть абсолютными. Результат — относительный путь с
/// ведущими `..` при необходимости (например, `../comp`).
fn relative_to(base: &Path, target: &Path) -> PathBuf {
    let base: Vec<_> = base.components().collect();
    let target: Vec<_> = target.components().collect();
    let mut common = 0;
    while common < base.len() && common < target.len() && base[common] == target[common] {
        common += 1;
    }
    let mut result = PathBuf::new();
    for _ in common..base.len() {
        result.push("..");
    }
    for component in &target[common..] {
        result.push(component.as_os_str());
    }
    result
}

/// Как записать зависимость компонента в `Cargo.toml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DependencyEntry {
    /// `name = { workspace = true, optional = true }`
    Workspace,
    /// `name = { path = "...", optional = true }`
    Path(PathBuf),
    /// `name = { version = "...", optional = true }`
    Version(String),
}

/// Проверяет доступность crates.io (используется в `doctor`).
#[cfg(feature = "network")]
pub fn ping_crates_io() -> bool {
    use std::time::Duration;
    let client = match reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
    {
        Ok(c) => c,
        Err(_) => return false,
    };
    client
        .get("https://crates.io/api/v1/crates/light")
        .header("User-Agent", "light-fw-cli (diagnostics)")
        .send()
        .is_ok()
}

/// Запрашивает последнюю версию крейта на crates.io.
///
/// Возвращает `Ok(None)`, если крейт не найден (HTTP 404).
#[cfg(feature = "network")]
pub fn fetch_crates_io(name: &str) -> Result<Option<semver::Version>> {
    use std::time::Duration;

    #[derive(serde::Deserialize)]
    struct CrateResponse {
        #[serde(rename = "crate")]
        krate: CrateInfo,
    }
    #[derive(serde::Deserialize)]
    struct CrateInfo {
        max_stable_version: Option<String>,
        max_version: String,
    }

    let url = format!("https://crates.io/api/v1/crates/{name}");
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()?;
    let response = client
        .get(&url)
        .header("User-Agent", "light-fw-cli (component manager)")
        .send()?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let response = response.error_for_status()?;
    let parsed: CrateResponse = response.json()?;
    let raw = parsed
        .krate
        .max_stable_version
        .unwrap_or(parsed.krate.max_version);
    let version = semver::Version::parse(&raw)?;
    Ok(Some(version))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("light-fw-cli-fetch-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn write_component(dir: &Path, name: &str) {
        fs::create_dir_all(dir.join("src")).expect("src");
        fs::write(dir.join("src/lib.rs"), "").expect("lib");
        fs::write(
            dir.join("Cargo.toml"),
            format!(
                "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
                 [package.metadata.light]\nkind = \"element\"\ndisplay-name = \"Test\"\ncore = \"^0.1\"\n\n\
                 [package.metadata.light.provides]\nelements = [\"checkbox\"]\n"
            ),
        )
        .expect("write component");
    }

    #[test]
    fn resolves_workspace_component() {
        let root = temp_dir("workspace");
        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"app\", \"comp\"]\n\n\
             [workspace.dependencies]\nlight-fw-element-form = { path = \"comp\", version = \"0.1.0\" }\n",
        )
        .expect("root manifest");
        let app = root.join("app");
        fs::create_dir_all(&app).expect("app");
        fs::write(
            app.join("Cargo.toml"),
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .expect("app manifest");
        write_component(&root.join("comp"), "light-fw-element-form");

        let catalog = Catalog::discover(&app, false);
        let resolved = catalog.resolve("light-fw-element-form").expect("resolve");
        assert_eq!(resolved.source, SourceKind::Workspace);
        assert_eq!(resolved.version, semver::Version::new(0, 1, 0));
        assert_eq!(resolved.provides.elements, vec!["checkbox"]);
        assert_eq!(
            catalog.dependency_entry(&resolved).expect("entry"),
            DependencyEntry::Workspace
        );
    }

    #[test]
    fn resolves_registry_fallback_without_network() {
        let dir = temp_dir("registry-fallback");
        fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n[workspace]\n",
        )
        .expect("manifest");
        let catalog = Catalog::discover(&dir, false);
        let resolved = catalog
            .resolve("light-fw-element-form")
            .expect("registry fallback");
        assert_eq!(resolved.source, SourceKind::CratesIo);
        assert_eq!(resolved.version, semver::Version::new(0, 4, 0));
    }

    #[test]
    fn offline_without_local_is_error() {
        let dir = temp_dir("offline");
        fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n[workspace]\n",
        )
        .expect("manifest");
        let catalog = Catalog::discover(&dir, true);
        let err = catalog
            .resolve("light-fw-element-form")
            .expect_err("offline error");
        assert!(matches!(err, Error::Offline(_)));
    }

    #[test]
    fn unknown_component_is_not_found() {
        let dir = temp_dir("unknown");
        fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n[workspace]\n",
        )
        .expect("manifest");
        let catalog = Catalog::discover(&dir, false);
        let err = catalog
            .resolve("light-fw-datepicker")
            .expect_err("not found");
        assert!(matches!(err, Error::ComponentNotFound(_)));
    }

    #[test]
    fn non_component_crate_name_is_rejected() {
        let dir = temp_dir("not-component");
        fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n[workspace]\n",
        )
        .expect("manifest");
        let catalog = Catalog::discover(&dir, false);
        let err = catalog.resolve("serde").expect_err("not a component");
        assert!(matches!(err, Error::NotComponent(_)));
    }

    struct MockRemote {
        name: &'static str,
        version: semver::Version,
    }

    impl RemoteRegistry for MockRemote {
        fn latest_version(&self, name: &str) -> Result<Option<semver::Version>> {
            if name == self.name {
                Ok(Some(self.version.clone()))
            } else {
                Ok(None)
            }
        }
    }

    #[test]
    fn resolves_external_component_from_remote() {
        let dir = temp_dir("external");
        fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n[workspace]\n",
        )
        .expect("manifest");
        let catalog = Catalog::with_remote(
            &dir,
            false,
            Box::new(MockRemote {
                name: "light-fw-datepicker",
                version: semver::Version::new(1, 2, 3),
            }),
        );
        let resolved = catalog
            .resolve("light-fw-datepicker")
            .expect("external component");
        assert_eq!(resolved.source, SourceKind::CratesIo);
        assert_eq!(resolved.version, semver::Version::new(1, 2, 3));
        assert_eq!(
            catalog.dependency_entry(&resolved).expect("entry"),
            DependencyEntry::Version("1.2.3".to_string())
        );
    }
}
