use std::path::Path;

use crate::error::Result;
use crate::fetch::is_component_crate_name;
use crate::manifest::{
    collect_dependency_details, find_workspace_root, read_workspace_dependencies,
    resolve_core_version as detect_core_version,
};
use crate::registry::ComponentRegistry;
use crate::scan::scan_templates;

use super::types::Lockfile;

/// Актуален ли существующий `light.lock` относительно `Cargo.toml`.
pub fn is_actual(project_root: &Path) -> Result<bool> {
    let path = Lockfile::path_in(project_root);
    if !path.is_file() {
        return Ok(false);
    }
    let current = Lockfile::read(project_root)?;
    let expected = Lockfile::from_project(project_root)?;
    Ok(current.same_components(&expected) && current.core == expected.core)
}
/// Читает установленные компоненты проекта: известные реестру и внешние
/// `light-fw-*` крейты.
pub(crate) fn collect_project_components(
    project_root: &Path,
) -> Result<Vec<crate::manifest::DepDetail>> {
    let cargo = project_root.join("Cargo.toml");
    let text = std::fs::read_to_string(&cargo)?;
    let details = collect_dependency_details(&text, project_root)?;
    let registry = ComponentRegistry::builtin();
    Ok(details
        .into_iter()
        .filter(|d| registry.by_name(&d.name).is_some() || is_component_crate_name(&d.name))
        .collect())
}

pub(crate) fn resolve_core_version(project_root: &Path) -> Result<String> {
    Ok(detect_core_version(project_root)?
        .map(|v| v.to_string())
        .unwrap_or_else(|| "0.0.0".to_string()))
}

pub(crate) fn relative_path(_base: &Path, path: &Path) -> String {
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
