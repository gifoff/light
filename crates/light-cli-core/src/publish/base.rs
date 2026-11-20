use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::{Error, Result};
use crate::manifest::{min_version, parse_manifest_from_text};
use crate::process::CommandRunner;

/// Аргументы `cargo metadata` для построения графа воркспейса.
pub fn metadata_args() -> Vec<String> {
    vec![
        "metadata".to_string(),
        "--no-deps".to_string(),
        "--format-version".to_string(),
        "1".to_string(),
    ]
}

/// Сырые данные `cargo metadata --no-deps --format-version 1`.
#[derive(Debug, Clone, Deserialize)]
pub struct Metadata {
    /// Пакеты воркспейса.
    pub packages: Vec<MetadataPackage>,
    /// Идентификаторы членов воркспейса.
    pub workspace_members: Vec<String>,
    /// Корень воркспейса.
    pub workspace_root: PathBuf,
}

/// Один пакет из `cargo metadata`.
#[derive(Debug, Clone, Deserialize)]
pub struct MetadataPackage {
    /// Имя пакета.
    pub name: String,
    /// Версия (строка semver).
    pub version: String,
    /// Уникальный идентификатор пакета.
    pub id: String,
    /// Путь к `Cargo.toml`.
    pub manifest_path: PathBuf,
    /// `None` — публикуется по умолчанию, `Some([])` — `publish = false`.
    pub publish: Option<Vec<String>>,
    /// Описание из манифеста.
    pub description: Option<String>,
    /// Лицензия из манифеста.
    pub license: Option<String>,
    /// Репозиторий из манифеста.
    pub repository: Option<String>,
    /// Readme из манифеста.
    pub readme: Option<String>,
    /// Целевые артефакты пакета (`bin`/`lib`/…).
    #[serde(default)]
    pub targets: Vec<MetadataTarget>,
    /// Зависимости пакета.
    pub dependencies: Vec<MetadataDependency>,
}

/// Один целевой артефакт пакета из `cargo metadata`.
#[derive(Debug, Clone, Deserialize)]
pub struct MetadataTarget {
    /// Имя цели (для бинарника — имя файла без расширения).
    pub name: String,
    /// Виды цели (`bin`, `lib`, `test`, …).
    #[serde(default)]
    pub kind: Vec<String>,
}

/// Одна зависимость из `cargo metadata`.
#[derive(Debug, Clone, Deserialize)]
pub struct MetadataDependency {
    /// Имя пакета зависимости.
    pub name: String,
    /// Требование версии (semver-range).
    pub req: String,
    /// `None` — обычная зависимость, `"dev"`, `"build"`.
    pub kind: Option<String>,
    /// Прямой путь для локальной зависимости.
    pub path: Option<PathBuf>,
}

/// Вид зависимости.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DepKind {
    /// Обычная зависимость.
    Normal,
    /// Зависимость сборки.
    Build,
    /// Зависимость разработки (crates.io её игнорирует).
    Dev,
}

impl DepKind {
    fn from_metadata(kind: Option<&str>) -> Self {
        match kind {
            Some("dev") => DepKind::Dev,
            Some("build") => DepKind::Build,
            _ => DepKind::Normal,
        }
    }
}

/// Внутренняя (workspace) зависимость пакета.
#[derive(Debug, Clone)]
pub struct InternalDep {
    /// Имя крейта-зависимости.
    pub name: String,
    /// Исходное требование версии.
    pub req_raw: String,
    /// Разобранное требование версии.
    pub req: semver::VersionReq,
    /// Вид зависимости.
    pub kind: DepKind,
}

/// Пакет воркспейса с разрешёнными внутренними зависимостями.
#[derive(Debug, Clone)]
pub struct PackageInfo {
    /// Имя крейта.
    pub name: String,
    /// Версия крейта.
    pub version: semver::Version,
    /// Путь к `Cargo.toml`.
    pub manifest_path: PathBuf,
    /// Публикуется ли крейт (`publish != false`).
    pub publishable: bool,
    /// Описание.
    pub description: Option<String>,
    /// Лицензия.
    pub license: Option<String>,
    /// Репозиторий.
    pub repository: Option<String>,
    /// Readme.
    pub readme: Option<String>,
    /// Внутренние зависимости (только члены воркспейса).
    pub deps: Vec<InternalDep>,
}

/// Воркспейс и его пакеты.
#[derive(Debug, Clone)]
pub struct Workspace {
    /// Корень воркспейса.
    pub root: PathBuf,
    /// Пакеты-члены воркспейса.
    pub packages: Vec<PackageInfo>,
    /// Внутренний индекс имён.
    pub(crate) by_name: BTreeMap<String, usize>,
}

impl Workspace {
    /// Строит модель воркспейса из данных `cargo metadata`.
    pub fn from_metadata(metadata: Metadata) -> Result<Self> {
        let members: BTreeSet<&str> = metadata
            .workspace_members
            .iter()
            .map(String::as_str)
            .collect();
        let member_packages: Vec<&MetadataPackage> = metadata
            .packages
            .iter()
            .filter(|package| members.contains(package.id.as_str()))
            .collect();
        let names: BTreeSet<String> = member_packages
            .iter()
            .map(|package| package.name.clone())
            .collect();

        let mut packages = Vec::with_capacity(member_packages.len());
        for package in member_packages {
            let version = semver::Version::parse(&package.version).map_err(|error| {
                Error::Publish(format!("некорректная версия у `{}`: {error}", package.name))
            })?;
            let mut deps = Vec::new();
            for dependency in &package.dependencies {
                if !names.contains(&dependency.name) {
                    continue;
                }
                let req = semver::VersionReq::parse(&dependency.req).map_err(|error| {
                    Error::Publish(format!(
                        "некорректное требование версии `{} = \"{}\"`: {error}",
                        dependency.name, dependency.req
                    ))
                })?;
                deps.push(InternalDep {
                    name: dependency.name.clone(),
                    req_raw: dependency.req.clone(),
                    req,
                    kind: DepKind::from_metadata(dependency.kind.as_deref()),
                });
            }
            packages.push(PackageInfo {
                name: package.name.clone(),
                version,
                manifest_path: package.manifest_path.clone(),
                publishable: package.publish.as_ref().is_none_or(|list| !list.is_empty()),
                description: package.description.clone(),
                license: package.license.clone(),
                repository: package.repository.clone(),
                readme: package.readme.clone(),
                deps,
            });
        }

        let mut by_name = BTreeMap::new();
        for (index, package) in packages.iter().enumerate() {
            by_name.insert(package.name.clone(), index);
        }

        Ok(Self {
            root: metadata.workspace_root,
            packages,
            by_name,
        })
    }

    /// Пакет по имени.
    pub fn package(&self, name: &str) -> Option<&PackageInfo> {
        self.by_name.get(name).map(|index| &self.packages[*index])
    }

    /// Пакет по имени или ошибка, если такого члена воркспейса нет.
    pub fn require(&self, name: &str) -> Result<&PackageInfo> {
        self.package(name)
            .ok_or_else(|| Error::Publish(format!("`{name}` не является членом воркспейса")))
    }
}

/// План публикации.
#[derive(Debug, Clone)]
pub struct PublishPlan {
    /// Крейты в топологическом порядке (зависимости — раньше зависимых).
    pub order: Vec<String>,
    /// Крейты, пропущенные из-за `publish = false`.
    pub skipped: Vec<String>,
}

/// Загружает `cargo metadata` для каталога `cwd`.
pub fn load_metadata(runner: &dyn CommandRunner, cwd: &Path) -> Result<Metadata> {
    let output = runner.run("cargo", &metadata_args(), Some(cwd))?;
    if !output.success() {
        return Err(Error::CommandFailed {
            program: "cargo metadata".to_string(),
            code: output.status,
            stderr: output.stderr.trim().to_string(),
        });
    }
    Ok(serde_json::from_str(&output.stdout)?)
}

/// Имя пакета, бинарник которого сейчас выполняется, если он лежит в `target/`
/// этого воркспейса.
///
/// Такой пакет исключается из pre-flight сборки ([`run_gates`]): иначе
/// `cargo build --workspace` пытается перелинковать запущенный файл и на Windows
/// падает с «failed to remove file …: access denied».
pub fn running_package_exclude(metadata: &Metadata) -> Option<String> {
    let exe = std::env::current_exe().ok()?.canonicalize().ok()?;
    let root = metadata.workspace_root.canonicalize().ok()?;
    if !exe.starts_with(root.join("target")) {
        return None;
    }
    let stem = exe.file_stem()?.to_str()?;

    let members: BTreeSet<&str> = metadata
        .workspace_members
        .iter()
        .map(String::as_str)
        .collect();
    metadata
        .packages
        .iter()
        .filter(|package| members.contains(package.id.as_str()))
        .find(|package| {
            package
                .targets
                .iter()
                .any(|target| target.kind.iter().any(|kind| kind == "bin") && target.name == stem)
        })
        .map(|package| package.name.clone())
}

/// Строит план публикации: порядок и пропущенные крейты.
///
/// При `only = None` — весь воркспейс; при `Some(name)` — только указанный
/// крейт (без цепочки). Возвращает ошибку, если публикуемый крейт зависит от
/// непубликуемого, или если найдены циклы.
pub fn plan(workspace: &Workspace, only: Option<&str>) -> Result<PublishPlan> {
    let skipped: Vec<String> = workspace
        .packages
        .iter()
        .filter(|package| !package.publishable)
        .map(|package| package.name.clone())
        .collect();
    let non_publishable: BTreeSet<&str> = workspace
        .packages
        .iter()
        .filter(|package| !package.publishable)
        .map(|package| package.name.as_str())
        .collect();

    for package in workspace.packages.iter().filter(|p| p.publishable) {
        for dep in package.deps.iter().filter(|d| d.kind != DepKind::Dev) {
            if non_publishable.contains(dep.name.as_str()) {
                return Err(Error::Publish(format!(
                    "публикуемый крейт `{}` зависит от `{}`, у которого `publish = false`",
                    package.name, dep.name
                )));
            }
        }
    }

    let order = match only {
        Some(name) => {
            let package = workspace.require(name)?;
            if !package.publishable {
                return Err(Error::Publish(format!(
                    "крейт `{name}` помечен `publish = false` и не может быть опубликован"
                )));
            }
            vec![name.to_string()]
        }
        None => topo_sort(workspace)?,
    };

    Ok(PublishPlan { order, skipped })
}

/// Топологическая сортировка публикуемых крейтов.
///
/// Среди готовых к публикации узлов выбирается первый по алфавиту — порядок
/// детерминирован и не зависит от порядка в `cargo metadata`.
fn topo_sort(workspace: &Workspace) -> Result<Vec<String>> {
    let selected: BTreeSet<&str> = workspace
        .packages
        .iter()
        .filter(|package| package.publishable)
        .map(|package| package.name.as_str())
        .collect();

    let mut indegree: BTreeMap<&str, usize> = selected.iter().map(|name| (*name, 0)).collect();
    let mut dependents: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for &name in &selected {
        let package = workspace.require(name)?;
        for dep in package.deps.iter().filter(|d| d.kind != DepKind::Dev) {
            if selected.contains(dep.name.as_str()) {
                if let Some(degree) = indegree.get_mut(name) {
                    *degree += 1;
                }
                dependents.entry(dep.name.as_str()).or_default().push(name);
            }
        }
    }

    let mut ready: BTreeSet<&str> = indegree
        .iter()
        .filter(|(_, degree)| **degree == 0)
        .map(|(name, _)| *name)
        .collect();
    let mut order = Vec::with_capacity(selected.len());
    while let Some(name) = ready.iter().next().copied() {
        ready.remove(name);
        order.push(name.to_string());
        if let Some(children) = dependents.get(name) {
            for child in children {
                if let Some(degree) = indegree.get_mut(child) {
                    *degree -= 1;
                    if *degree == 0 {
                        ready.insert(child);
                    }
                }
            }
        }
    }

    if order.len() != selected.len() {
        let cycle: Vec<String> = indegree
            .iter()
            .filter(|(_, degree)| **degree > 0)
            .map(|(name, _)| (*name).to_string())
            .collect();
        return Err(Error::Publish(format!(
            "цикл зависимостей среди крейтов: {}",
            cycle.join(", ")
        )));
    }

    Ok(order)
}

/// Проверка непубликуемого крейта, от которого зависит публикуемый — уже в
/// [`plan`]; здесь остаются версии.
///
/// Проверяет, что `version` в `[dependencies]` совпадает с публикуемой версией
/// соответствующего workspace-крейта.
pub fn check_version_consistency(workspace: &Workspace, plan: &PublishPlan) -> Result<()> {
    for name in &plan.order {
        let package = workspace.require(name)?;
        for dep in package.deps.iter().filter(|d| d.kind != DepKind::Dev) {
            let Some(target) = workspace.package(&dep.name) else {
                continue;
            };
            if !target.publishable {
                continue;
            }
            let min = min_version(&dep.req);
            let consistent =
                dep.req.matches(&target.version) && min.as_ref() == Some(&target.version);
            if !consistent {
                return Err(Error::Publish(format!(
                    "`{name}` зависит от `{} = \"{}\"`, а версия workspace — {}; версии должны совпадать",
                    dep.name, dep.req_raw, target.version
                )));
            }
        }
    }
    Ok(())
}

/// Предупреждения о неполных метаданных crates.io (не блокируют публикацию).
pub fn metadata_warnings(workspace: &Workspace, plan: &PublishPlan) -> Vec<String> {
    let mut warnings = Vec::new();
    for name in &plan.order {
        let Some(package) = workspace.package(name) else {
            continue;
        };
        let mut missing = Vec::new();
        if package.description.as_deref().is_none_or(str::is_empty) {
            missing.push("description");
        }
        if package.license.as_deref().is_none_or(str::is_empty) {
            missing.push("license");
        }
        if package.repository.as_deref().is_none_or(str::is_empty) {
            missing.push("repository");
        }
        if package.readme.as_deref().is_none_or(str::is_empty) {
            missing.push("readme");
        }
        if !missing.is_empty() {
            warnings.push(format!("{name}: missing {}", missing.join(", ")));
        }
    }
    warnings
}

/// Валидирует `[package.metadata.light]` каждого публикуемого крейта.
pub fn validate_manifests(workspace: &Workspace, plan: &PublishPlan) -> Result<()> {
    for name in &plan.order {
        let package = workspace.require(name)?;
        let text = std::fs::read_to_string(&package.manifest_path)?;
        parse_manifest_from_text(&text)
            .map_err(|error| Error::Publish(format!("{name}: {error}")))?;
    }
    Ok(())
}

/// Прогоняет `light check --strict` по `light-demo`, если он есть в воркспейсе.
///
/// Возвращает `Ok(Some(message))` при найденных проблемах.
pub fn check_demo(workspace: &Workspace) -> Result<Option<String>> {
    let Some(demo) = workspace.package("light-demo") else {
        return Ok(None);
    };
    let Some(dir) = demo.manifest_path.parent() else {
        return Ok(None);
    };
    let report = crate::project::analyze_project(dir)?;
    let problems =
        report.scan.errors.len() + report.missing.len() + report.unused.len() + report.issues.len();
    if problems > 0 {
        Ok(Some(format!(
            "light check --strict: light-demo содержит {problems} проблем(ы)"
        )))
    } else {
        Ok(None)
    }
}
