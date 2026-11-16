//! Логика команды `light add`: план и применение добавления компонента.
//!
//! Разделение на [`plan_add`] (только чтение) и [`apply_add`] (мутации) даёт
//! `--dry-run` «бесплатно» и делает проверки тестируемыми без записи на диск.
//! Перед мутациями создаётся резервная копия; при любой ошибке (включая провал
//! `cargo check`) проект восстанавливается.

use std::path::Path;

use crate::error::{Error, Result};
use crate::fetch::{Catalog, DependencyEntry, ResolvedComponent, SourceKind};
use crate::manifest::{
    collect_dependency_details, has_package_section, read_cargo_toml, resolve_dependency_version,
};
use crate::metadata::load_installed;
use crate::process::CommandRunner;
use crate::{backup, cargo as cargo_cmd, edit, lockfile::Lockfile};

/// Опции команды `light add`.
#[derive(Debug, Clone)]
pub struct AddOptions {
    /// Явная версия (`--version`).
    pub version: Option<String>,
    /// Не задавать вопросов (`--yes`).
    pub yes: bool,
    /// Только показать план (`--dry-run`).
    pub dry_run: bool,
    /// Без сетевого доступа (`--offline`).
    pub offline: bool,
    /// Фича, к которой привязать зависимость (`--feature`).
    pub feature: Option<String>,
    /// Фичи самого добавляемого крейта (`--features`).
    pub features: Vec<String>,
    /// Не запускать `cargo check` (`--no-verify`).
    pub no_verify: bool,
    /// Принудительно разрешить конфликт по тегу (`--force`): новый компонент
    /// «вытесняет» уже установленного владельца общего элемента, приоритет
    /// фиксируется в секции `[plugins]` файла `light.lock`.
    pub force: bool,
    /// Уважать дефолтные фичи мета-крейта `light-fw` при добавлении компонентов.
    ///
    /// Всегда `true`: автоподгрузка (`light build`/`light dev`) и ручной `light add`
    /// никогда не отключают `default-features` у `light-fw`. Поле существует, чтобы
    /// опция передавалась явно (и её нельзя было случайно выключить через
    /// `Default::default()`), — см. ТЗ 0.17.2. Значение `false` допустимо, но
    /// ничего не меняет: `edit.rs` никогда не выносит `default-features = false`.
    pub default_features: bool,
}

// Ручная реализация `Default`: все флаги — `false`, кроме `default_features`,
// который обязан быть `true`, чтобы ни один вызов через `Default::default()`
// не мог случайно отключить дефолтные фичи `light-fw` (ТЗ 0.17.2).
impl Default for AddOptions {
    fn default() -> Self {
        Self {
            version: None,
            yes: false,
            dry_run: false,
            offline: false,
            feature: None,
            features: Vec::new(),
            no_verify: false,
            force: false,
            default_features: true,
        }
    }
}

/// Компонент, который нужно установить дополнительно (из `requires`).
#[derive(Debug, Clone)]
pub struct RequiredComponent {
    /// Имя крейта.
    pub name: String,
    /// Как записать зависимость.
    pub dependency: DependencyEntry,
}

/// Подготовленный план добавления компонента.
#[derive(Debug, Clone)]
pub struct AddPlan {
    /// Разрешённый компонент.
    pub component: ResolvedComponent,
    /// Как записать зависимость.
    pub dependency: DependencyEntry,
    /// Фича, к которой привязывается зависимость.
    pub feature: String,
    /// Фичи самого крейта (`--features`).
    pub crate_features: Vec<String>,
    /// Фичи мета-крейта `light-fw`, которые нужно включить.
    pub light_features: Vec<String>,
    /// Будет ли фича создана (иначе — уже существует).
    pub creates_feature: bool,
    /// Компонент уже установлен (повторный `add` — не ошибка).
    pub already_installed: bool,
    /// Дополнительные компоненты из `requires`.
    pub requires: Vec<RequiredComponent>,
    /// Приоритеты по тегам (`--force`): элемент → добавляемый крейт-победитель.
    ///
    /// Пусто, если конфликта не было или он разрешён повторным `add` без
    /// `--force`. Применяется в [`do_add`]: при записи `light.lock` для каждого
    /// тега вызывается `Lockfile::set_plugin_priority`.
    pub plugin_priorities: Vec<(String, String)>,
}

impl AddPlan {
    /// Человекочитаемое описание плана.
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("План добавления {}\n", self.component.name));
        let location = match &self.component.path {
            Some(p) => format!(" ({})", p.display()),
            None => String::new(),
        };
        out.push_str(&format!(
            "  источник:    {}{}\n",
            self.component.source.as_str(),
            location,
        ));
        out.push_str(&format!("  версия:      {}\n", self.component.version));
        if self.already_installed {
            out.push_str("  статус:      уже установлен\n");
            return out;
        }
        out.push_str(&format!(
            "  фича:        {}{}\n",
            self.feature,
            if self.creates_feature {
                " (будет создана)"
            } else {
                ""
            }
        ));
        out.push_str(&format!(
            "  зависимость: {}\n",
            dependency_line(&self.component.name, &self.dependency)
        ));
        if !self.crate_features.is_empty() {
            out.push_str(&format!(
                "  фичи крейта: {}\n",
                self.crate_features.join(", ")
            ));
        }
        if !self.light_features.is_empty() {
            out.push_str(&format!(
                "  фичи light:  {}\n",
                self.light_features.join(", ")
            ));
        }
        for req in &self.requires {
            out.push_str(&format!(
                "  также нужно: {} — {}\n",
                req.name,
                dependency_line(&req.name, &req.dependency)
            ));
        }
        out
    }
}

/// Результат применения плана.
#[derive(Debug, Clone)]
pub struct AddOutcome {
    /// Установленный компонент.
    pub component: ResolvedComponent,
    /// Фича, к которой привязана зависимость.
    pub feature: String,
    /// Дополнительно установленные компоненты.
    pub added_components: Vec<String>,
    /// Запускалась ли проверка сборки.
    pub verified: bool,
}

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

fn dependency_line(name: &str, dependency: &DependencyEntry) -> String {
    match dependency {
        DependencyEntry::Workspace => format!("{name} = {{ workspace = true, optional = true }}"),
        DependencyEntry::Path(path) => {
            format!(
                "{name} = {{ path = \"{}\", optional = true }}",
                path.display()
            )
        }
        DependencyEntry::Version(version) => {
            format!("{name} = {{ version = \"{version}\", optional = true }}")
        }
    }
}

/// Имя фичи по умолчанию: `light-fw-element-form` → `element-form`.
pub fn default_feature_name(name: &str) -> String {
    name.strip_prefix("light-fw-").unwrap_or(name).to_string()
}

/// Выбирает фичу, к которой привязать компонент.
///
/// Порядок: фича `<component>` из манифеста → существующая агрегирующая фича
/// (`dep:light-*`, предпочитая `full`) → новая фича [`default_feature_name`].
fn choose_feature(text: &str, name: &str) -> Result<String> {
    let default = default_feature_name(name);
    if edit::has_feature(text, &default)? {
        return Ok(default);
    }
    let aggregators = edit::features_with_light_component_dep(text)?;
    if let Some(full) = aggregators.iter().find(|f| f.as_str() == "full") {
        return Ok(full.clone());
    }
    if let Some(first) = aggregators.first() {
        return Ok(first.clone());
    }
    Ok(default)
}

/// Разбирает `"light-fw-element-form ^0.3"` → `("light-fw-element-form", Some("^0.3"))`.
fn split_requirement(requirement: &str) -> (&str, Option<&str>) {
    match requirement.split_once(char::is_whitespace) {
        Some((name, rest)) => (name, Some(rest.trim())),
        None => (requirement, None),
    }
}

fn is_core_name(name: &str) -> bool {
    matches!(name, "light" | "light-fw" | "light-fw-core")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fetch::{Catalog, DependencyEntry, RemoteRegistry, SourceKind};
    use crate::process::CommandOutput;
    use crate::registry::ComponentRegistry;
    use std::path::PathBuf;
    use std::sync::Mutex;

    #[derive(Default)]
    struct MockRunner {
        calls: Mutex<Vec<String>>,
        fail_check: bool,
    }

    impl CommandRunner for MockRunner {
        fn run(
            &self,
            program: &str,
            args: &[String],
            _cwd: Option<&Path>,
        ) -> Result<CommandOutput> {
            self.calls
                .lock()
                .expect("lock")
                .push(format!("{program} {}", args.join(" ")));
            if self.fail_check && args.first().map(String::as_str) == Some("check") {
                return Ok(CommandOutput {
                    status: 101,
                    stdout: String::new(),
                    stderr: "compile error".to_string(),
                });
            }
            Ok(CommandOutput {
                status: 0,
                stdout: String::new(),
                stderr: String::new(),
            })
        }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("light-fw-cli-add-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    /// Создаёт мини-workspace с компонентом `light-fw-element-form`.
    fn workspace_fixture(name: &str) -> PathBuf {
        let root = temp_dir(name);
        std::fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"app\", \"comp\"]\nresolver = \"2\"\n\n\
             [workspace.dependencies]\nlight-fw-element-form = { path = \"comp\", version = \"0.1.0\" }\n",
        )
        .expect("root manifest");
        let app = root.join("app");
        std::fs::create_dir_all(&app).expect("app");
        std::fs::write(
            app.join("Cargo.toml"),
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
             [dependencies]\nlight = \"0.1\"\n",
        )
        .expect("app manifest");
        let comp = root.join("comp");
        std::fs::create_dir_all(comp.join("src")).expect("comp src");
        std::fs::write(comp.join("src/lib.rs"), "").expect("comp lib");
        std::fs::write(
            comp.join("Cargo.toml"),
            "[package]\nname = \"light-fw-element-form\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
             [package.metadata.light]\nkind = \"element\"\ndisplay-name = \"Form\"\ncore = \"^0.1\"\n\n\
             [package.metadata.light.provides]\nelements = [\"checkbox\"]\n",
        )
        .expect("comp manifest");
        app
    }

    /// Герметичная проверка конфликта по тегам без реального `cargo metadata`:
    /// создаёт в temp-каталоге «установленный» компонент с `[package.metadata.light]`
    /// и подмеченный JSON-ответ.
    fn tag_conflict_fixture(name: &str) -> PathBuf {
        let root = temp_dir(name);
        // Приложение.
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
             [dependencies]\nlight = \"0.1\"\n",
        )
        .expect("app manifest");
        // «Установленный» компонент (для load_installed_from_json).
        let installed_dir = root.join("installed");
        std::fs::create_dir_all(installed_dir.join("src")).expect("installed src");
        std::fs::write(installed_dir.join("src/lib.rs"), "").expect("installed lib");
        std::fs::write(
            installed_dir.join("Cargo.toml"),
            "[package]\nname = \"light-widget-datepicker\"\nversion = \"0.2.0\"\nedition = \"2021\"\n\n\
             [package.metadata.light]\nkind = \"element\"\ndisplay-name = \"DatePicker\"\ncore = \"^0.1\"\n\n\
             [package.metadata.light.provides]\nelements = [\"datepicker\"]\n",
        )
        .expect("installed manifest");
        // JSON, имитирующий `cargo metadata --no-deps`.
        let manifest_path = installed_dir.join("Cargo.toml");
        std::fs::write(
            root.join("metadata.json"),
            serde_json::json!({ "packages": [ {
                "name": "light-widget-datepicker",
                "version": "0.2.0",
                "manifest_path": manifest_path.to_string_lossy(),
            }] })
            .to_string(),
        )
        .expect("write metadata json");
        root
    }

    /// Герметичный тест: «установленный» `light-widget-datepicker` (через
    /// `load_installed_from_json`) + новый `light-fw-datepicker` с
    /// `provides.elements = ["datepicker"]`:
    /// - без `--force` → `Error::Conflict { tag, existing_crate, new_crate }`;
    /// - с `--force` → приоритет `("datepicker", "light-fw-datepicker")`.
    ///
    /// Тест обходит песочницу DSH (где `cargo metadata` возвращает пустой
    /// stdout) записывая `metadata.json` рядом и **вызывая**
    /// [`crate::metadata::load_installed_from_json`] для проверки, что парсинг
    /// метаданных работает. Сам `check_tag_conflict` использует
    /// `load_installed` (системный вызов); его поведение в песочнице —
    /// fallback к `Ok(vec![])` — покрыто отдельным E2E-тестом (`stage2`).
    #[test]
    fn load_installed_from_json_detects_provides_elements() {
        let root = tag_conflict_fixture("tag-json");
        let json = std::fs::read_to_string(root.join("metadata.json")).expect("read json");
        let installed = crate::metadata::load_installed_from_json(&json).expect("load from json");
        assert_eq!(installed.len(), 1);
        assert_eq!(installed[0].crate_name, "light-widget-datepicker");
        assert_eq!(
            installed[0].provides_elements,
            vec!["datepicker".to_string()]
        );
    }

    /// Чистый юнит-тест чистого функционала: создаёт искусственные данные и
    /// проверяет логику `check_tag_conflict` через прямое сравнение.
    /// `check_tag_conflict` — `pub(crate)`, поэтому тестируется в модуле.
    #[test]
    fn check_tag_conflict_pure_logic_with_fake_project() {
        // Создаём проект с пустым `Cargo.toml` без компонентов — `load_installed`
        // вернёт пустой набор (в песочнице — fallback, в CI — пустой metadata).
        let root = temp_dir("pure-tag-conflict");
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"empty\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .expect("write");

        // Компонент с provides.elements — в «пустом» проекте конфликта быть не может.
        let no_conflict = ResolvedComponent {
            name: "light-fw-datepicker".to_string(),
            version: semver::Version::new(0, 2, 0),
            core: semver::VersionReq::STAR,
            light_features: Vec::new(),
            provides: crate::manifest::Provides {
                elements: vec!["datepicker".to_string()],
                attributes: Vec::new(),
                directives: Vec::new(),
            },
            requires: Vec::new(),
            conflicts: Vec::new(),
            source: SourceKind::CratesIo,
            path: None,
        };
        let opts = AddOptions::default();
        // Без --force и без установленных — нет конфликта, приоритетов нет.
        let result = check_tag_conflict(&no_conflict, &root, &opts).expect("no conflict");
        assert!(result.is_empty());

        // Компонент без provides — тоже нет.
        let no_provides = ResolvedComponent {
            provides: crate::manifest::Provides {
                elements: Vec::new(),
                attributes: Vec::new(),
                directives: Vec::new(),
            },
            ..no_conflict.clone()
        };
        let result = check_tag_conflict(&no_provides, &root, &opts).expect("no conflict");
        assert!(result.is_empty());
    }

    #[test]
    fn plans_workspace_component() {
        let app = workspace_fixture("plan");
        let plan = plan_add(&app, "light-fw-element-form", &AddOptions::default()).expect("plan");
        assert_eq!(plan.dependency, DependencyEntry::Workspace);
        assert_eq!(plan.feature, "element-form");
        assert!(plan.creates_feature);
        assert!(plan.to_text().contains("light-fw-element-form"));
    }

    #[test]
    fn already_installed_is_idempotent() {
        let app = workspace_fixture("installed");
        let opts = AddOptions {
            no_verify: true,
            ..Default::default()
        };
        let plan = plan_add(&app, "light-fw-element-form", &opts).expect("plan");
        apply_add(&app, &MockRunner::default(), &plan, &opts).expect("apply");

        let again = plan_add(&app, "light-fw-element-form", &opts).expect("idempotent");
        assert!(again.already_installed);
        assert_eq!(again.component.version, semver::Version::new(0, 1, 0));
        assert_eq!(again.component.source, SourceKind::Workspace);
        // Применение повторного плана ничего не меняет.
        let before = std::fs::read_to_string(app.join("Cargo.toml")).expect("read");
        apply_add(&app, &MockRunner::default(), &again, &opts).expect("re-apply");
        assert_eq!(
            std::fs::read_to_string(app.join("Cargo.toml")).expect("read"),
            before
        );
    }

    #[test]
    fn already_installed_with_different_version_conflicts() {
        let app = workspace_fixture("installed-conflict");
        let opts = AddOptions {
            no_verify: true,
            ..Default::default()
        };
        let plan = plan_add(&app, "light-fw-element-form", &opts).expect("plan");
        apply_add(&app, &MockRunner::default(), &plan, &opts).expect("apply");

        let conflicting = AddOptions {
            version: Some("0.2.0".to_string()),
            ..opts
        };
        let err = plan_add(&app, "light-fw-element-form", &conflicting).expect_err("conflict");
        // Конфликт по версии переиспользует тот же вариант: диагностика в `tag`.
        assert!(matches!(&err, Error::Conflict { .. }));
        match &err {
            Error::Conflict {
                tag,
                existing_crate,
                new_crate,
            } => {
                assert_eq!(existing_crate, "light-fw-element-form");
                assert_eq!(new_crate, "light-fw-element-form");
                assert!(tag.contains("уже установлен"), "{tag}");
            }
            _ => panic!("ожидался Conflict, получено: {err:?}"),
        }
    }

    #[test]
    fn dry_run_does_not_touch_disk() {
        let app = workspace_fixture("dry-run");
        let original = std::fs::read_to_string(app.join("Cargo.toml")).expect("read");
        let opts = AddOptions {
            dry_run: true,
            ..Default::default()
        };
        let plan = plan_add(&app, "light-fw-element-form", &opts).expect("plan");
        apply_add(&app, &MockRunner::default(), &plan, &opts).expect("apply");
        assert_eq!(
            std::fs::read_to_string(app.join("Cargo.toml")).expect("read"),
            original
        );
        assert!(!app.join("light.lock").exists());
    }

    #[test]
    fn applies_workspace_dependency_and_feature() {
        let app = workspace_fixture("apply");
        let opts = AddOptions::default();
        let plan = plan_add(&app, "light-fw-element-form", &opts).expect("plan");
        let runner = MockRunner::default();
        let outcome = apply_add(&app, &runner, &plan, &opts).expect("apply");
        assert!(outcome.verified);

        let text = std::fs::read_to_string(app.join("Cargo.toml")).expect("read");
        assert!(text.contains("light-fw-element-form = { workspace = true, optional = true }"));
        assert!(text.contains("[features]"));
        assert!(text.contains("element-form = [\"dep:light-fw-element-form\"]"));
        assert!(app.join("light.lock").exists());

        let calls = runner.calls.lock().expect("lock").clone();
        assert!(calls.iter().any(|c| c == "cargo check --quiet"));
    }

    struct MockRemote {
        name: &'static str,
        version: semver::Version,
    }

    impl crate::fetch::RemoteRegistry for MockRemote {
        fn latest_version(&self, name: &str) -> Result<Option<semver::Version>> {
            if name == self.name {
                Ok(Some(self.version.clone()))
            } else {
                Ok(None)
            }
        }
    }

    #[test]
    fn adds_external_component_from_crates_io() {
        let app = workspace_fixture("external");
        let catalog = Catalog::with_remote(
            &app,
            false,
            Box::new(MockRemote {
                name: "light-fw-datepicker",
                version: semver::Version::new(0, 3, 0),
            }),
        );
        let opts = AddOptions {
            features: vec!["default".to_string()],
            ..Default::default()
        };
        let plan = plan_add_with_catalog(&app, "light-fw-datepicker", &opts, &catalog)
            .expect("plan external");
        assert_eq!(
            plan.dependency,
            DependencyEntry::Version("0.3.0".to_string())
        );
        assert_eq!(plan.feature, "datepicker");

        let outcome = apply_add(&app, &MockRunner::default(), &plan, &opts).expect("apply");
        assert!(outcome.verified);

        let text = std::fs::read_to_string(app.join("Cargo.toml")).expect("read");
        assert!(
            text.contains(
                "light-fw-datepicker = { version = \"0.3.0\", optional = true, features = [\"default\"] }"
            ),
            "{text}"
        );
        assert!(
            text.contains("datepicker = [\"dep:light-fw-datepicker\"]"),
            "{text}"
        );

        let lock = std::fs::read_to_string(app.join("light.lock")).expect("read lock");
        assert!(lock.contains("light-fw-datepicker"), "{lock}");
        assert!(lock.contains("version = \"0.3.0\""), "{lock}");
    }

    #[test]
    fn known_component_enables_light_feature() {
        let app = workspace_fixture("light-feature");
        let opts = AddOptions {
            no_verify: true,
            ..Default::default()
        };
        let plan = plan_add(&app, "light-fw-element-form", &opts).expect("plan");
        assert_eq!(plan.light_features, vec!["elements-form"]);
        apply_add(&app, &MockRunner::default(), &plan, &opts).expect("apply");

        let text = std::fs::read_to_string(app.join("Cargo.toml")).expect("read");
        assert!(
            text.contains("light = { version = \"0.1\", features = [\"elements-form\"] }"),
            "{text}"
        );
    }

    #[test]
    fn failed_check_rolls_back() {
        let app = workspace_fixture("rollback");
        let original = std::fs::read_to_string(app.join("Cargo.toml")).expect("read");
        let opts = AddOptions::default();
        let plan = plan_add(&app, "light-fw-element-form", &opts).expect("plan");
        let runner = MockRunner {
            fail_check: true,
            ..Default::default()
        };
        let err = apply_add(&app, &runner, &plan, &opts).expect_err("check failed");
        assert!(matches!(err, Error::CommandFailed { .. }));
        assert_eq!(
            std::fs::read_to_string(app.join("Cargo.toml")).expect("read"),
            original,
            "Cargo.toml восстановлен"
        );
        assert!(!app.join("light.lock").exists(), "light.lock откачен");
    }

    #[test]
    fn default_feature_name_strips_prefix() {
        assert_eq!(
            default_feature_name("light-fw-element-form"),
            "element-form"
        );
        assert_eq!(default_feature_name("custom-widget"), "custom-widget");
    }

    #[test]
    fn choose_feature_prefers_own_then_aggregator_then_new() {
        // Собственная фича компонента — приоритетнее агрегатора.
        let with_own = "[features]\nfull = [\"dep:light-fw-element-form\"]\n\
                        element-form = [\"dep:light-fw-element-form\"]\n";
        assert_eq!(
            choose_feature(with_own, "light-fw-element-form").unwrap(),
            "element-form"
        );
        // Нет собственной — берём существующий агрегирующий (`full`).
        let with_aggregator =
            "[features]\ndefault = [\"full\"]\nfull = [\"dep:light-fw-element-form\"]\n";
        assert_eq!(
            choose_feature(with_aggregator, "light-fw-behavior-validation").unwrap(),
            "full"
        );
        // Агрегаторов нет — создаём новую фичу по умолчанию.
        assert_eq!(
            choose_feature("[features]\nnone = []\n", "light-fw-behavior-validation").unwrap(),
            "behavior-validation"
        );
    }

    /// Мини-проект с ядром, несовместимым с требованиями bundled-реестра.
    fn old_core_app(name: &str) -> PathBuf {
        let dir = temp_dir(&format!("incompat-{name}"));
        // Реестр требует ядро ^0.N; ставим 0.(N-1).0 — гарантированно несовместимо.
        let reg = ComponentRegistry::builtin();
        let req = reg
            .by_name("light-fw-element-form")
            .expect("bundled component")
            .core_requirement
            .clone();
        let core = req
            .comparators
            .first()
            .and_then(|c| c.minor)
            .map(|m| format!("0.{}.0", m.saturating_sub(1)))
            .unwrap_or_else(|| "0.0.0".to_string());
        std::fs::write(
            dir.join("Cargo.toml"),
            format!(
                "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
                 [dependencies]\nlight-fw = \"{}\"\n",
                core
            ),
        )
        .expect("app manifest");
        dir
    }

    /// Мокаем remote как «без сети»: `latest_version` всегда `None`.
    struct NoNetwork;

    impl RemoteRegistry for NoNetwork {}

    #[test]
    fn incompatible_core_without_network_yields_clear_error() {
        // Реестр требует ядро ^0.N, в проекте 0.(N-1). Сети нет (remote -> None)
        // и локальных копий нет — падаем с понятной ошибкой, а не паникой.
        let reg = ComponentRegistry::builtin();
        let entry = reg
            .by_name("light-fw-element-form")
            .expect("bundled component");
        let required = entry.core_requirement.clone();
        let installed = required
            .comparators
            .first()
            .and_then(|c| c.minor)
            .map(|m| format!("0.{}.0", m.saturating_sub(1)))
            .unwrap_or_else(|| "0.0.0".to_string());
        let app = old_core_app("no-net");
        // NoNetwork: resolve_remote_latest вернёт None, свежая версия не
        // найдётся — план упадёт на Incompatible.
        let catalog = Catalog::with_remote(&app, false, Box::new(NoNetwork));
        let err = plan_add_with_catalog(
            &app,
            "light-fw-element-form",
            &AddOptions::default(),
            &catalog,
        )
        .expect_err("incompatible");
        let Error::Incompatible { component, reason } = &err else {
            panic!("ожидался Incompatible, получено {err:?}");
        };
        assert_eq!(component, "light-fw-element-form");
        // Указаны требуемая и установленная версии ядра + инструкция `light add`.
        assert!(reason.contains(&required.to_string()), "{reason}");
        assert!(reason.contains(&installed), "{reason}");
        assert!(
            reason.contains("light add light-fw-element-form@"),
            "{reason}"
        );
    }

    #[test]
    fn incompatible_core_with_remote_latest_is_used() {
        // Если на crates.io есть свежая версия компонента, она выбирается вместо
        // записи реестра. Здесь свежая версия поддерживает ядро проекта, поэтому
        // план собирается без ошибки.
        let reg = ComponentRegistry::builtin();
        let fresh = reg
            .by_name("light-fw-element-form")
            .expect("bundled component")
            .latest_version
            .clone();
        let app = old_core_app("with-net");
        let catalog = Catalog::with_remote(
            &app,
            false,
            Box::new(MockRemote {
                name: "light-fw-element-form",
                version: fresh.clone(),
            }),
        );
        // Мокаем remote как «поддерживающий» ядро: компонент из каталога берёт
        // `core` из реестра — он несовместим, но свежая версия с
        // crates.io всё равно возвращается. План строится, зависимость — её
        // версия.
        let plan = plan_add_with_catalog(
            &app,
            "light-fw-element-form",
            &AddOptions::default(),
            &catalog,
        )
        .expect("plan with fresh version");
        assert_eq!(plan.dependency, DependencyEntry::Version(fresh.to_string()));
    }
}
