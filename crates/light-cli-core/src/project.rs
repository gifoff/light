use std::collections::BTreeSet;
use std::path::Path;

use crate::error::Result;
use crate::manifest::{collect_dependencies, read_cargo_toml};
use crate::registry::ComponentRegistry;
use crate::report::{Issue, ProjectReport, Recommendation};
use crate::resolve::needed_components;
use crate::scan::{ScanError, scan_templates};

/// Анализирует проект в каталоге `project_root`.
///
/// Читает `Cargo.toml`, сканирует шаблоны и сопоставляет использование
/// компонентов с установленными зависимостями. Ничего не пишет на диск.
pub fn analyze_project(project_root: &Path) -> Result<ProjectReport> {
    let cargo_path = project_root.join("Cargo.toml");
    let cargo_toml = read_cargo_toml(&cargo_path)?;
    let dependencies = collect_dependencies(&cargo_toml)?;

    let scan = scan_templates(project_root)?;
    let registry = ComponentRegistry::builtin();
    let needed = needed_components(&scan, &registry);

    // «Установлено» — только те зависимости, которые реестр знает как компоненты.
    let installed: Vec<_> = dependencies
        .iter()
        .filter(|dep| registry.by_name(&dep.name).is_some())
        .cloned()
        .collect();
    let installed_names: BTreeSet<&str> = installed.iter().map(|c| c.name.as_str()).collect();

    let missing: Vec<_> = needed
        .iter()
        .filter(|n| !installed_names.contains(n.name.as_str()))
        .cloned()
        .collect();

    let needed_names: BTreeSet<&str> = needed.iter().map(|n| n.name.as_str()).collect();
    let unused: Vec<_> = installed
        .iter()
        .filter(|c| !needed_names.contains(c.name.as_str()))
        .cloned()
        .collect();

    let issues: Vec<Issue> = scan
        .errors
        .iter()
        .map(|ScanError { file, message }| Issue {
            message: format!("{}: {message}", file.display()),
        })
        .collect();

    let recommendations: Vec<Recommendation> = missing
        .iter()
        .map(|m| {
            let requirement = registry
                .by_name(&m.name)
                .map(|c| c.requirement())
                .unwrap_or_else(|| m.name.clone());
            Recommendation {
                name: m.name.clone(),
                requirement,
            }
        })
        .collect();

    Ok(ProjectReport {
        scan,
        installed,
        missing,
        unused,
        issues,
        recommendations,
    })
}
