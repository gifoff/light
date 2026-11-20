use std::path::Path;

use super::helpers::{choose_feature, is_core_name, split_requirement};
use super::types::{AddOptions, AddPlan, RequiredComponent};
use crate::edit;
use crate::error::{Error, Result};
use crate::fetch::{Catalog, DependencyEntry, ResolvedComponent, SourceKind};
use crate::manifest::{
    collect_dependency_details, has_package_section, read_cargo_toml, resolve_dependency_version,
};
use crate::metadata::load_installed;

/// Строит план добавления компонента `name` в проект `project_root`.
pub fn plan_add(project_root: &Path, name: &str, opts: &AddOptions) -> Result<AddPlan> {
    let catalog = Catalog::discover(project_root, opts.offline);
    plan_add_with_catalog(project_root, name, opts, &catalog)
}

/// Проверяет конфликт по тегам элемента: новый компонент не должен «вытеснять»
/// уже установленного владельца общего элемента без `--force`.
///
/// Возврат:
/// - [`Error::Conflict`] — есть общий элемент с установленным компонентом и
///   не включён `--force`; структура (`tag`, `existing_crate`, `new_crate`)
///   используется CLI для интерактивного промпта.
/// - `Ok(Vec<(tag, new_crate)>)` — список приоритетов, которые нужно записать
///   в секцию `[plugins]` файла `light.lock` (только при `--force`).
///
/// Сравнение идёт по `provides.elements` — тем же данным, что использует
/// `light check` для построения отчёта о конфликтах (см. [`crate::check`]).
pub(crate) fn check_tag_conflict(
    component: &ResolvedComponent,
    project_root: &Path,
    opts: &AddOptions,
) -> Result<Vec<(String, String)>> {
    if component.provides.elements.is_empty() {
        return Ok(Vec::new());
    }
    let installed = load_installed(project_root)?;
    let mut priorities = Vec::new();
    for tag in &component.provides.elements {
        let existing = installed
            .iter()
            .filter(|m| m.crate_name != component.name)
            .find(|m| m.provides_elements.iter().any(|e| e == tag));
        match existing {
            Some(provider) if !opts.force => {
                return Err(Error::Conflict {
                    tag: tag.clone(),
                    existing_crate: provider.crate_name.clone(),
                    new_crate: component.name.clone(),
                });
            }
            Some(_provider) => {
                // --force: новый компонент вытесняет старого владельца тега;
                // приоритет фиксируется на стороне CLI через do_add.
                priorities.push((tag.clone(), component.name.clone()));
            }
            None => {}
        }
    }
    Ok(priorities)
}

/// Строит план с явно заданным каталогом (подменяемый crates.io для тестов).
pub fn plan_add_with_catalog(
    project_root: &Path,
    name: &str,
    opts: &AddOptions,
    catalog: &Catalog,
) -> Result<AddPlan> {
    let cargo_path = project_root.join("Cargo.toml");
    if !cargo_path.is_file() {
        return Err(Error::Manifest(format!(
            "Cargo.toml не найден в {}",
            project_root.display()
        )));
    }
    let text = read_cargo_toml(&cargo_path)?;
    if !has_package_section(&text)? {
        return Err(Error::Manifest(
            "Cargo.toml не содержит секции [package]: укажите каталог приложения".to_string(),
        ));
    }

    let installed = collect_dependency_details(&text, project_root)?;
    let installed_names: Vec<&str> = installed.iter().map(|d| d.name.as_str()).collect();

    if installed_names.contains(&name) {
        return already_installed_plan(project_root, &text, name, opts, catalog);
    }

    let mut component = catalog.resolve(name)?;
    if let Some(version) = &opts.version {
        component.version = semver::Version::parse(version)?;
    }

    // Обёртка переименована: новый проект зависит от `light-fw`; старые
    // (опубликованные) проекты могут всё ещё использовать `light` — обе
    // версии дают версию ядра.
    let core = crate::manifest::resolve_core_version(project_root)?;
    if let Some(core) = &core
        && !component.core.matches(core)
    {
        // Ядро проекта несовместимо с версией из реестра (например, проект на
        // 0.16, а реестр требует ^0.17). Не падаем сразу: пробуем найти свежую
        // совместимую версию компонента — сначала в локальном workspace/vendor,
        // затем на crates.io (через `remote`, без сети возвращает `None`).
        // Если ничего нет — выдаём понятную ошибку с инструкцией.
        let fresh = catalog
            .resolve_local(name)?
            .and_then(|c| c.core.matches(core).then_some(c))
            .or_else(|| catalog.resolve_remote_latest(name).ok().flatten());
        component = match fresh {
            Some(fresh) => fresh,
            None => {
                let mv = opts
                    .version
                    .clone()
                    .unwrap_or_else(|| component.version.to_string());
                return Err(Error::Incompatible {
                    component: name.to_string(),
                    reason: format!(
                        "Компонент {name} требует ядро {}, но в проекте {core}. \
                         Обновите проект до версии ядра, совместимой с {}, \
                         или установите компонент вручную: light add {name}@{mv}",
                        component.core, component.core,
                    ),
                });
            }
        };
    }

    // Этап 2 Plugin API: проверка по `conflicts` (имена крейтов из манифеста) —
    // новый компонент явно несовместим с установленным крейтом.
    for conflict in &component.conflicts {
        if installed_names.iter().any(|n| n == conflict) {
            return Err(Error::Conflict {
                tag: conflict.clone(),
                existing_crate: conflict.clone(),
                new_crate: name.to_string(),
            });
        }
    }

    // Конфликт по тегам элемента (`provides.elements`): новый «вытесняет»
    // установленного владельца, если только не включён `--force`. При `--force`
    // получаем список приоритетов для записи в light.lock секцию [plugins].
    let plugin_priorities = check_tag_conflict(&component, project_root, opts)?;

    let feature = match &opts.feature {
        Some(feature) => feature.clone(),
        None => choose_feature(&text, name)?,
    };
    let creates_feature = !edit::has_feature(&text, &feature)?;

    let dependency = catalog.dependency_entry(&component)?;

    let mut requires = Vec::new();
    for requirement in &component.requires {
        let (dep_name, _) = split_requirement(requirement);
        if is_core_name(dep_name) || installed_names.contains(&dep_name) {
            continue;
        }
        if requires
            .iter()
            .any(|r: &RequiredComponent| r.name == dep_name)
        {
            continue;
        }
        let resolved = catalog.resolve(dep_name)?;
        requires.push(RequiredComponent {
            name: dep_name.to_string(),
            dependency: catalog.dependency_entry(&resolved)?,
        });
    }

    let light_features = component.light_features.clone();
    Ok(AddPlan {
        component,
        dependency,
        crate_features: opts.features.clone(),
        light_features,
        feature,
        creates_feature,
        already_installed: false,
        requires,
        plugin_priorities,
    })
}

/// План для уже установленного компонента: ничего не меняет, но и не ошибка.
fn already_installed_plan(
    project_root: &Path,
    text: &str,
    name: &str,
    opts: &AddOptions,
    catalog: &Catalog,
) -> Result<AddPlan> {
    let current = resolve_dependency_version(project_root, name)?;
    if let (Some(requested), Some(current)) = (&opts.version, &current)
        && current.to_string() != *requested
    {
        // Повторный `add` с явной версией ≠ текущей. Это не «конфликт по
        // тегу» (см. [`Error::Conflict`]), но переиспользуем тот же вариант
        // для согласованности вывода: `tag` содержит всю диагностику.
        return Err(Error::Conflict {
            tag: format!("{name} уже установлен (версия {current}), запрошена {requested}"),
            existing_crate: name.to_string(),
            new_crate: name.to_string(),
        });
    }
    let version = current.unwrap_or_else(|| semver::Version::new(0, 0, 0));
    let info = catalog.registry().by_name(name);
    let local = catalog.resolve_local(name)?;
    let (source, path) = match &local {
        Some(resolved) => (resolved.source, resolved.path.clone()),
        None => (SourceKind::CratesIo, None),
    };
    let component = ResolvedComponent {
        name: name.to_string(),
        version: version.clone(),
        core: info
            .map(|i| i.core_requirement.clone())
            .unwrap_or(semver::VersionReq::STAR),
        light_features: info.map(|i| i.light_features.clone()).unwrap_or_default(),
        provides: info.map(|i| i.provides.clone()).unwrap_or_default(),
        requires: Vec::new(),
        conflicts: Vec::new(),
        source,
        path,
    };
    let feature = match &opts.feature {
        Some(feature) => feature.clone(),
        None => choose_feature(text, name)?,
    };
    Ok(AddPlan {
        component,
        dependency: DependencyEntry::Version(version.to_string()),
        feature,
        crate_features: opts.features.clone(),
        light_features: info.map(|i| i.light_features.clone()).unwrap_or_default(),
        creates_feature: false,
        already_installed: true,
        requires: Vec::new(),
        plugin_priorities: Vec::new(),
    })
}
