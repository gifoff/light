use std::path::{Path, PathBuf};

use super::helpers::DependencyEntry;
use super::helpers::{is_component_crate_name, relative_to};
use super::types::{CratesIoRegistry, RemoteRegistry, ResolvedComponent, SourceKind};
use crate::error::{Error, Result};
use crate::manifest::{
    DepDetail, Provides, absolute, find_workspace_root, parse_manifest_from_text, read_cargo_toml,
    read_package_name_from_text, read_package_version, read_workspace_dependencies,
};
use crate::registry::ComponentRegistry;
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

    /// Последняя версия компонента на crates.io (через `remote`), без учёта
    /// локальных workspace/vendor. Используется при несовместимости реестра с
    /// ядром проекта: берём свежую версию, которая может уже поддерживать это
    /// ядро. Без фичи `network` возвращает `None` (сеть недоступна).
    pub fn resolve_remote_latest(&self, name: &str) -> Result<Option<ResolvedComponent>> {
        if self.offline {
            return Ok(None);
        }
        let info = self.registry.by_name(name);
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
                Ok(Some(ResolvedComponent {
                    name: name.to_string(),
                    version,
                    core,
                    light_features,
                    provides,
                    requires: Vec::new(),
                    conflicts: Vec::new(),
                    source: SourceKind::CratesIo,
                    path: None,
                }))
            }
            None => Ok(None),
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
