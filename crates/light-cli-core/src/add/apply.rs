use std::path::Path;

use super::types::{AddOptions, AddOutcome, AddPlan};
use crate::error::Result;
use crate::fetch::DependencyEntry;
use crate::manifest::read_cargo_toml;
use crate::process::CommandRunner;
use crate::{backup, cargo as cargo_cmd, edit, lockfile::Lockfile};

/// Применяет план: мутирует проект и (при необходимости) проверяет сборку.
pub fn apply_add(
    project_root: &Path,
    runner: &dyn CommandRunner,
    plan: &AddPlan,
    opts: &AddOptions,
) -> Result<AddOutcome> {
    if opts.dry_run || plan.already_installed {
        return Ok(AddOutcome {
            component: plan.component.clone(),
            feature: plan.feature.clone(),
            added_components: plan.requires.iter().map(|r| r.name.clone()).collect(),
            verified: false,
        });
    }

    let snapshot = backup::create(project_root)?;
    match do_add(project_root, runner, plan, opts) {
        Ok(outcome) => Ok(outcome),
        Err(error) => {
            backup::restore(&snapshot)?;
            Err(error)
        }
    }
}

fn do_add(
    project_root: &Path,
    runner: &dyn CommandRunner,
    plan: &AddPlan,
    opts: &AddOptions,
) -> Result<AddOutcome> {
    let cargo_path = project_root.join("Cargo.toml");

    add_dependency(
        project_root,
        &plan.component.name,
        &plan.dependency,
        &plan.crate_features,
    )?;
    let mut added_components = Vec::new();
    for req in &plan.requires {
        add_dependency(project_root, &req.name, &req.dependency, &[])?;
        added_components.push(req.name.clone());
    }

    let text = read_cargo_toml(&cargo_path)?;
    let mut doc = edit::parse(&text)?;
    // Фичи ставятся на обёртку: новый ключ `light-fw`, старый `light` — для
    // уже опубликованных проектов (обратная совместимость).
    if edit::has_dependency(&doc, "light-fw") {
        edit::add_features_to_dependency(&mut doc, "light-fw", &plan.light_features)?;
    } else if edit::has_dependency(&doc, "light") {
        edit::add_features_to_dependency(&mut doc, "light", &plan.light_features)?;
    }
    edit::add_dep_to_feature(&mut doc, &plan.feature, &plan.component.name)?;
    for req in &plan.requires {
        edit::add_dep_to_feature(&mut doc, &plan.feature, &req.name)?;
    }
    std::fs::write(&cargo_path, doc.to_string())?;

    // Этап 2 Plugin API: если `--force` «вытеснил» прежних владельцев общих
    // элементов, фиксируем приоритеты в секции [plugins] light.lock.
    // from_project уже переносит существующие приоритеты — дописываем новые.
    let mut lock = Lockfile::from_project(project_root)?;
    for (tag, winner) in &plan.plugin_priorities {
        lock.set_plugin_priority(tag, winner);
    }
    lock.write(project_root)?;

    let verified = !opts.no_verify;
    if verified {
        cargo_cmd::run_check(runner, project_root, false)?;
    }

    Ok(AddOutcome {
        component: plan.component.clone(),
        feature: plan.feature.clone(),
        added_components,
        verified,
    })
}

fn add_dependency(
    project_root: &Path,
    name: &str,
    dependency: &DependencyEntry,
    features: &[String],
) -> Result<()> {
    let cargo_path = project_root.join("Cargo.toml");
    let text = read_cargo_toml(&cargo_path)?;
    let mut doc = edit::parse(&text)?;
    match dependency {
        DependencyEntry::Workspace => edit::set_workspace_optional(&mut doc, name)?,
        DependencyEntry::Path(path) => {
            edit::set_path_optional(&mut doc, name, &path.to_string_lossy())?
        }
        DependencyEntry::Version(version) => edit::set_version_optional(&mut doc, name, version)?,
    }
    edit::add_features_to_dependency(&mut doc, name, features)?;
    std::fs::write(&cargo_path, doc.to_string())?;
    Ok(())
}
