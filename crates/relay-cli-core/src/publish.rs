//! Команда `relay publish`: оркестрация публикации крейтов воркспейса.
//!
//! Граф строится из `cargo metadata --no-deps`: модуль не парсит `Cargo.toml`
//! руками для топологии. Внешние эффекты (запуск `cargo`/`git`, сеть, ожидание)
//! спрятаны за трейтами [`CommandRunner`], [`RegistryProbe`] и [`Sleeper`],
//! поэтому логика графа, ретраев и проверки версий тестируется без процессов и
//! без сети.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

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
    by_name: BTreeMap<String, usize>,
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

/// Валидирует `[package.metadata.relay]` каждого публикуемого крейта.
pub fn validate_manifests(workspace: &Workspace, plan: &PublishPlan) -> Result<()> {
    for name in &plan.order {
        let package = workspace.require(name)?;
        let text = std::fs::read_to_string(&package.manifest_path)?;
        parse_manifest_from_text(&text)
            .map_err(|error| Error::Publish(format!("{name}: {error}")))?;
    }
    Ok(())
}

/// Прогоняет `relay check --strict` по `relay-demo`, если он есть в воркспейсе.
///
/// Возвращает `Ok(Some(message))` при найденных проблемах.
pub fn check_demo(workspace: &Workspace) -> Result<Option<String>> {
    let Some(demo) = workspace.package("relay-demo") else {
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
            "relay check --strict: relay-demo содержит {problems} проблем(ы)"
        )))
    } else {
        Ok(None)
    }
}

/// Результат одного гейта pre-flight.
#[derive(Debug, Clone)]
pub struct GateOutcome {
    /// Имя гейта (`build`/`test`/`clippy`/`fmt`).
    pub name: String,
    /// Успешно ли.
    pub ok: bool,
    /// stderr при неудаче.
    pub stderr: String,
}

/// Гейты pre-flight с опциональным исключением пакета.
///
/// `exclude` — имя пакета, который не нужно пересобирать (например, только что
/// запущенный бинарник `relay`); `cargo fmt` исключение не поддерживает.
pub fn gate_commands(exclude: Option<&str>) -> Vec<(&'static str, Vec<String>)> {
    fn args(base: &[&str], exclude: Option<&str>) -> Vec<String> {
        let mut args: Vec<String> = base.iter().map(|arg| (*arg).to_string()).collect();
        if let Some(name) = exclude {
            args.push("--exclude".to_string());
            args.push(name.to_string());
        }
        args
    }

    let mut clippy = args(&["clippy", "--workspace"], exclude);
    clippy.push("--".to_string());
    clippy.push("-D".to_string());
    clippy.push("warnings".to_string());

    vec![
        ("build", args(&["build", "--workspace"], exclude)),
        ("test", args(&["test", "--workspace"], exclude)),
        ("clippy", clippy),
        ("fmt", args(&["fmt", "--check", "--all"], None)),
    ]
}

/// Прогоняет гейты `build`/`test`/`clippy`/`fmt`, останавливаясь на первом сбое.
pub fn run_gates(
    runner: &dyn CommandRunner,
    root: &Path,
    exclude: Option<&str>,
) -> Result<Vec<GateOutcome>> {
    let mut outcomes = Vec::new();
    for (name, args) in gate_commands(exclude) {
        let output = runner.run("cargo", &args, Some(root))?;
        let ok = output.success();
        outcomes.push(GateOutcome {
            name: name.to_string(),
            ok,
            stderr: output.stderr.trim().to_string(),
        });
        if !ok {
            break;
        }
    }
    Ok(outcomes)
}

/// Аргументы `cargo package --list` для проверки упаковки в режиме dry-run.
pub fn package_list_args(name: &str) -> Vec<String> {
    vec![
        "package".to_string(),
        "--list".to_string(),
        "-p".to_string(),
        name.to_string(),
        "--allow-dirty".to_string(),
    ]
}

/// Проверяет упаковку крейта через `cargo package --list`.
pub fn package_crate(runner: &dyn CommandRunner, root: &Path, name: &str) -> Result<()> {
    let output = runner.run("cargo", &package_list_args(name), Some(root))?;
    if !output.success() {
        return Err(Error::CommandFailed {
            program: format!("cargo package -p {name}"),
            code: output.status,
            stderr: output.stderr.trim().to_string(),
        });
    }
    Ok(())
}

/// Параметры вызова `cargo publish`.
#[derive(Debug, Clone, Default)]
pub struct PublishInvocation {
    /// Полная верификация (без `--no-verify`).
    pub verify: bool,
    /// Разрешить грязное рабочее дерево.
    pub allow_dirty: bool,
    /// Токен реестра.
    pub token: Option<String>,
}

/// Аргументы `cargo publish` для крейта.
pub fn publish_args(name: &str, invocation: &PublishInvocation) -> Vec<String> {
    let mut args = vec!["publish".to_string(), "-p".to_string(), name.to_string()];
    if !invocation.verify {
        args.push("--no-verify".to_string());
    }
    if invocation.allow_dirty {
        args.push("--allow-dirty".to_string());
    }
    if let Some(token) = &invocation.token {
        args.push("--token".to_string());
        args.push(token.clone());
    }
    args
}

/// Причина сбоя `cargo publish`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureKind {
    /// Индекс crates.io ещё не увидел предыдущий крейт — стоит повторить.
    IndexStale,
    /// Прочая ошибка — ретраи не помогут.
    Other,
}

/// Паттерны stderr, означающие «индекс ещё не обновился».
///
/// Список сознательно вынесен в константу и легко расширяется.
pub const INDEX_STALE_PATTERNS: &[&str] = &[
    "failed to select a version",
    "no matching package named",
    "could not be found in registry index",
];

/// Классифицирует stderr `cargo publish` (exit code всегда 101, поэтому — по тексту).
pub fn classify_publish_failure(stderr: &str) -> FailureKind {
    let lower = stderr.to_lowercase();
    if INDEX_STALE_PATTERNS
        .iter()
        .any(|pattern| lower.contains(pattern))
    {
        FailureKind::IndexStale
    } else {
        FailureKind::Other
    }
}

/// Настройки ретраев на устаревание индекса.
#[derive(Debug, Clone, Copy)]
pub struct RetryConfig {
    /// Сколько раз повторить попытку (не считая первой).
    pub max_retries: u32,
    /// Базовая задержка.
    pub base: Duration,
    /// Множитель экспоненциального backoff.
    pub factor: u32,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 6,
            base: Duration::from_secs(5),
            factor: 2,
        }
    }
}

impl RetryConfig {
    /// Читает настройки из окружения (`RELAY_PUBLISH_RETRY_*`).
    ///
    /// - `RELAY_PUBLISH_RETRY_MAX` — число повторов (по умолчанию 6);
    /// - `RELAY_PUBLISH_RETRY_BASE_MS` — базовая задержка в мс (по умолчанию 5000);
    /// - `RELAY_PUBLISH_RETRY_FACTOR` — множитель (по умолчанию 2).
    pub fn from_env() -> Self {
        let mut config = Self::default();
        if let Ok(value) = std::env::var("RELAY_PUBLISH_RETRY_MAX")
            && let Ok(parsed) = value.parse()
        {
            config.max_retries = parsed;
        }
        if let Ok(value) = std::env::var("RELAY_PUBLISH_RETRY_BASE_MS")
            && let Ok(parsed) = value.parse::<u64>()
        {
            config.base = Duration::from_millis(parsed);
        }
        if let Ok(value) = std::env::var("RELAY_PUBLISH_RETRY_FACTOR")
            && let Ok(parsed) = value.parse::<u32>()
            && parsed >= 1
        {
            config.factor = parsed;
        }
        config
    }

    /// Задержка перед повтором номер `retry_index` (с нуля).
    pub fn delay(&self, retry_index: u32) -> Duration {
        let mut delay = self.base;
        for _ in 0..retry_index {
            delay = delay.saturating_mul(self.factor);
        }
        delay
    }
}

/// Ожидание между ретраями (инжектируется, чтобы тесты не спали).
pub trait Sleeper: Send + Sync {
    /// Спит `duration`.
    fn sleep(&self, duration: Duration);
}

/// Реальный [`Sleeper`] на `std::thread::sleep`.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemSleeper;

impl Sleeper for SystemSleeper {
    fn sleep(&self, duration: Duration) {
        std::thread::sleep(duration);
    }
}

/// Итог публикации одного крейта.
#[derive(Debug, Clone)]
pub struct PublishAttempt {
    /// Успешно ли опубликован.
    pub success: bool,
    /// Число сделанных повторов.
    pub retries: u32,
    /// stderr последней неудачной попытки.
    pub stderr: String,
}

/// Публикует крейт с ретраями на устаревание индекса crates.io.
///
/// Ретраи делаются только для [`FailureKind::IndexStale`]; прочие ошибки
/// возвращаются сразу.
pub fn publish_with_retries(
    runner: &dyn CommandRunner,
    sleeper: &dyn Sleeper,
    root: &Path,
    name: &str,
    invocation: &PublishInvocation,
    retry: &RetryConfig,
) -> Result<PublishAttempt> {
    let args = publish_args(name, invocation);
    let mut retries = 0;
    loop {
        let output = runner.run("cargo", &args, Some(root))?;
        if output.success() {
            return Ok(PublishAttempt {
                success: true,
                retries,
                stderr: String::new(),
            });
        }
        let stderr = output.stderr;
        if classify_publish_failure(&stderr) == FailureKind::Other || retries >= retry.max_retries {
            return Ok(PublishAttempt {
                success: false,
                retries,
                stderr,
            });
        }
        sleeper.sleep(retry.delay(retries));
        retries += 1;
    }
}

/// Итог проверки crates.io API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiOutcome {
    /// Реестр вернул список версий.
    Versions(Vec<semver::Version>),
    /// Крейт в реестре не найден.
    NotFound,
    /// Реестр недоступен (нет сети, ошибка API).
    Unavailable,
}

/// Источник данных об опубликованных версиях crates.io.
pub trait RegistryProbe: Send + Sync {
    /// Возвращает список версий крейта на crates.io.
    fn versions(&self, name: &str) -> ApiOutcome;
}

/// Реальный опрос crates.io API.
#[derive(Debug, Default, Clone, Copy)]
pub struct CratesIoProbe;

impl RegistryProbe for CratesIoProbe {
    fn versions(&self, name: &str) -> ApiOutcome {
        #[cfg(feature = "network")]
        {
            crates_io_versions(name)
        }
        #[cfg(not(feature = "network"))]
        {
            let _ = name;
            ApiOutcome::Unavailable
        }
    }
}

#[cfg(feature = "network")]
fn crates_io_versions(name: &str) -> ApiOutcome {
    #[derive(Deserialize)]
    struct Response {
        versions: Vec<VersionEntry>,
    }
    #[derive(Deserialize)]
    struct VersionEntry {
        num: String,
    }

    let client = match reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
    {
        Ok(client) => client,
        Err(_) => return ApiOutcome::Unavailable,
    };
    let url = format!("https://crates.io/api/v1/crates/{name}");
    let response = match client
        .get(&url)
        .header("User-Agent", "relay-cli (publish pre-flight)")
        .send()
    {
        Ok(response) => response,
        Err(_) => return ApiOutcome::Unavailable,
    };
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return ApiOutcome::NotFound;
    }
    let response = match response.error_for_status() {
        Ok(response) => response,
        Err(_) => return ApiOutcome::Unavailable,
    };
    match response.json::<Response>() {
        Ok(parsed) => ApiOutcome::Versions(
            parsed
                .versions
                .iter()
                .filter_map(|entry| semver::Version::parse(&entry.num).ok())
                .collect(),
        ),
        Err(_) => ApiOutcome::Unavailable,
    }
}

/// Локальные git-теги (используются как fallback к crates.io API).
pub fn git_tags(runner: &dyn CommandRunner, root: &Path) -> Option<BTreeSet<String>> {
    let output = runner
        .run(
            "git",
            &["tag".to_string(), "--list".to_string()],
            Some(root),
        )
        .ok()?;
    if !output.success() {
        return None;
    }
    Some(
        output
            .stdout
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect(),
    )
}

/// Результат проверки «версия уже опубликована».
#[derive(Debug, Clone)]
pub struct PublishedCheck {
    /// Уже опубликована ли версия.
    pub published: bool,
    /// Предупреждения (расхождение источников, недоступность API).
    pub warnings: Vec<String>,
}

/// Сверяет версию с crates.io API и git-тегами `v<version>`.
///
/// Оба источника «уже есть» → публикация запрещена; расхождение — warning и
/// продолжение; недоступность API — fallback на git-теги с warning.
pub fn check_already_published(
    probe: &dyn RegistryProbe,
    git_tags: Option<&BTreeSet<String>>,
    name: &str,
    version: &semver::Version,
) -> PublishedCheck {
    let api = match probe.versions(name) {
        ApiOutcome::Versions(versions) => Some(versions.contains(version)),
        ApiOutcome::NotFound => Some(false),
        ApiOutcome::Unavailable => None,
    };
    let git = git_tags.map(|tags| tags.contains(&format!("v{version}")));

    let mut warnings = Vec::new();
    let published = match (api, git) {
        (Some(true), Some(true)) => true,
        (Some(false), Some(false)) => false,
        (Some(api), Some(git)) if api != git => {
            warnings.push(format!(
                "crates.io API и git-теги расходятся для {name} v{version}; продолжаю"
            ));
            false
        }
        (None, Some(true)) => {
            warnings.push(format!(
                "crates.io API unreachable, falling back to git tags for {name}"
            ));
            true
        }
        (None, Some(false)) => {
            warnings.push(format!(
                "crates.io API unreachable, falling back to git tags for {name}"
            ));
            false
        }
        (Some(true), None) => {
            warnings.push(format!(
                "git-теги недоступны, доверяю crates.io API для {name}"
            ));
            true
        }
        (Some(false), None) => {
            warnings.push(format!(
                "git-теги недоступны, доверяю crates.io API для {name}"
            ));
            false
        }
        (None, None) => {
            warnings.push(format!(
                "crates.io API недоступен и git-теги недоступны для {name}; проверить не удалось"
            ));
            false
        }
        (Some(_), Some(_)) => unreachable!("случай равенства обработан выше"),
    };

    PublishedCheck {
        published,
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::CommandOutput;
    use std::collections::VecDeque;
    use std::sync::Mutex;

    fn package(name: &str, publishable: bool, deps: &[(&str, DepKind, &str)]) -> PackageInfo {
        PackageInfo {
            name: name.to_string(),
            version: semver::Version::new(0, 2, 0),
            manifest_path: PathBuf::from(format!("{name}/Cargo.toml")),
            publishable,
            description: None,
            license: None,
            repository: None,
            readme: None,
            deps: deps
                .iter()
                .map(|(dep_name, kind, req)| InternalDep {
                    name: (*dep_name).to_string(),
                    req_raw: (*req).to_string(),
                    req: semver::VersionReq::parse(req).expect("req"),
                    kind: *kind,
                })
                .collect(),
        }
    }

    fn workspace(packages: Vec<PackageInfo>) -> Workspace {
        let mut by_name = BTreeMap::new();
        for (index, package) in packages.iter().enumerate() {
            by_name.insert(package.name.clone(), index);
        }
        Workspace {
            root: PathBuf::from("."),
            packages,
            by_name,
        }
    }

    fn ok_output() -> CommandOutput {
        CommandOutput {
            status: 0,
            stdout: String::new(),
            stderr: String::new(),
        }
    }

    fn fail_output(stderr: &str) -> CommandOutput {
        CommandOutput {
            status: 101,
            stdout: String::new(),
            stderr: stderr.to_string(),
        }
    }

    struct ScriptedRunner {
        outputs: Mutex<VecDeque<CommandOutput>>,
        calls: Mutex<Vec<Vec<String>>>,
    }

    impl ScriptedRunner {
        fn new(outputs: Vec<CommandOutput>) -> Self {
            Self {
                outputs: Mutex::new(outputs.into()),
                calls: Mutex::new(Vec::new()),
            }
        }
        fn call_count(&self) -> usize {
            self.calls.lock().expect("calls").len()
        }
    }

    impl CommandRunner for ScriptedRunner {
        fn run(
            &self,
            _program: &str,
            args: &[String],
            _cwd: Option<&Path>,
        ) -> Result<CommandOutput> {
            self.calls.lock().expect("calls").push(args.to_vec());
            Ok(self
                .outputs
                .lock()
                .expect("outputs")
                .pop_front()
                .expect("scripted output"))
        }
    }

    struct RecordingSleeper {
        sleeps: Mutex<Vec<Duration>>,
    }

    impl RecordingSleeper {
        fn new() -> Self {
            Self {
                sleeps: Mutex::new(Vec::new()),
            }
        }
        fn sleeps(&self) -> Vec<Duration> {
            self.sleeps.lock().expect("sleeps").clone()
        }
    }

    impl Sleeper for RecordingSleeper {
        fn sleep(&self, duration: Duration) {
            self.sleeps.lock().expect("sleeps").push(duration);
        }
    }

    struct MockProbe {
        outcome: ApiOutcome,
    }

    impl RegistryProbe for MockProbe {
        fn versions(&self, _name: &str) -> ApiOutcome {
            self.outcome.clone()
        }
    }

    #[test]
    fn topological_order_puts_dependencies_first() {
        let ws = workspace(vec![
            package(
                "relay-cli",
                true,
                &[("relay-cli-core", DepKind::Normal, "^0.2.0")],
            ),
            package("relay-core", true, &[]),
            package(
                "relay-cli-core",
                true,
                &[("relay-core", DepKind::Normal, "^0.2.0")],
            ),
        ]);
        let plan = plan(&ws, None).expect("plan");
        assert_eq!(
            plan.order,
            vec!["relay-core", "relay-cli-core", "relay-cli"]
        );
    }

    #[test]
    fn dev_dependencies_do_not_create_edges() {
        let ws = workspace(vec![
            package("a", true, &[]),
            package("b", true, &[("a", DepKind::Dev, "^0.2.0")]),
        ]);
        let plan = plan(&ws, None).expect("plan");
        assert_eq!(plan.order, vec!["a", "b"]);
    }

    #[test]
    fn publish_false_is_skipped() {
        let ws = workspace(vec![
            package("relay-core", true, &[]),
            package(
                "relay-demo",
                false,
                &[("relay-core", DepKind::Normal, "^0.2.0")],
            ),
        ]);
        let plan = plan(&ws, None).expect("plan");
        assert_eq!(plan.order, vec!["relay-core"]);
        assert_eq!(plan.skipped, vec!["relay-demo"]);
    }

    #[test]
    fn publishable_depending_on_unpublished_is_error() {
        let ws = workspace(vec![
            package("app", true, &[("dep", DepKind::Normal, "^0.2.0")]),
            package("dep", false, &[]),
        ]);
        let error = plan(&ws, None).expect_err("должна быть ошибка");
        assert!(
            error.to_string().contains("publish = false"),
            "неожиданная ошибка: {error}"
        );
    }

    #[test]
    fn dependency_cycle_is_error() {
        let ws = workspace(vec![
            package("a", true, &[("b", DepKind::Normal, "^0.2.0")]),
            package("b", true, &[("a", DepKind::Normal, "^0.2.0")]),
        ]);
        let error = plan(&ws, None).expect_err("цикл");
        assert!(error.to_string().contains("цикл"), "{error}");
    }

    #[test]
    fn single_crate_selects_one() {
        let ws = workspace(vec![
            package("a", true, &[]),
            package("b", true, &[("a", DepKind::Normal, "^0.2.0")]),
        ]);
        let plan = plan(&ws, Some("b")).expect("plan");
        assert_eq!(plan.order, vec!["b"]);
    }

    #[test]
    fn unknown_single_crate_is_error() {
        let ws = workspace(vec![package("a", true, &[])]);
        let error = plan(&ws, Some("missing")).expect_err("не найден");
        assert!(error.to_string().contains("не является членом"), "{error}");
    }

    #[test]
    fn single_unpublished_crate_is_error() {
        let ws = workspace(vec![package("demo", false, &[])]);
        let error = plan(&ws, Some("demo")).expect_err("publish = false");
        assert!(error.to_string().contains("publish = false"), "{error}");
    }

    #[test]
    fn version_consistency_detects_mismatch() {
        let ws = workspace(vec![
            package("core", true, &[]),
            package("comp", true, &[("core", DepKind::Normal, "^0.1.0")]),
        ]);
        let plan = plan(&ws, None).expect("plan");
        let error = check_version_consistency(&ws, &plan).expect_err("mismatch");
        assert!(error.to_string().contains("должны совпадать"), "{error}");
    }

    #[test]
    fn version_consistency_accepts_matching_dependency() {
        let ws = workspace(vec![
            package("core", true, &[]),
            package("comp", true, &[("core", DepKind::Normal, "^0.2.0")]),
        ]);
        let plan = plan(&ws, None).expect("plan");
        assert!(check_version_consistency(&ws, &plan).is_ok());
    }

    #[test]
    fn metadata_warnings_list_missing_fields() {
        let ws = workspace(vec![package("bare", true, &[]), {
            let mut p = package("full", true, &[]);
            p.description = Some("d".into());
            p.license = Some("MIT".into());
            p.repository = Some("https://example.com".into());
            p.readme = Some("README.md".into());
            p
        }]);
        let plan = plan(&ws, None).expect("plan");
        let warnings = metadata_warnings(&ws, &plan);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].starts_with("bare: missing "), "{warnings:?}");
        assert!(warnings[0].contains("readme"), "{warnings:?}");
    }

    #[test]
    fn classify_detects_index_staleness() {
        assert_eq!(
            classify_publish_failure("error: failed to select a version for the requirement"),
            FailureKind::IndexStale
        );
        assert_eq!(
            classify_publish_failure("no matching package named `relay-core` found"),
            FailureKind::IndexStale
        );
        assert_eq!(
            classify_publish_failure("error: api token not provided"),
            FailureKind::Other
        );
    }

    #[test]
    fn retry_backoff_is_exponential() {
        let config = RetryConfig {
            max_retries: 4,
            base: Duration::from_secs(5),
            factor: 2,
        };
        assert_eq!(config.delay(0), Duration::from_secs(5));
        assert_eq!(config.delay(1), Duration::from_secs(10));
        assert_eq!(config.delay(2), Duration::from_secs(20));
        assert_eq!(config.delay(3), Duration::from_secs(40));
    }

    #[test]
    fn publish_retries_index_staleness_then_succeeds() {
        let runner = ScriptedRunner::new(vec![
            fail_output("no matching package named `relay-core` found"),
            ok_output(),
        ]);
        let sleeper = RecordingSleeper::new();
        let config = RetryConfig {
            max_retries: 3,
            base: Duration::from_secs(5),
            factor: 2,
        };
        let attempt = publish_with_retries(
            &runner,
            &sleeper,
            Path::new("."),
            "relay",
            &PublishInvocation::default(),
            &config,
        )
        .expect("publish");
        assert!(attempt.success);
        assert_eq!(attempt.retries, 1);
        assert_eq!(sleeper.sleeps(), vec![Duration::from_secs(5)]);
        assert_eq!(runner.call_count(), 2);
    }

    #[test]
    fn publish_stops_immediately_on_other_error() {
        let runner = ScriptedRunner::new(vec![fail_output("error: api token not provided")]);
        let sleeper = RecordingSleeper::new();
        let attempt = publish_with_retries(
            &runner,
            &sleeper,
            Path::new("."),
            "relay",
            &PublishInvocation::default(),
            &RetryConfig::default(),
        )
        .expect("publish");
        assert!(!attempt.success);
        assert_eq!(attempt.retries, 0);
        assert!(sleeper.sleeps().is_empty());
        assert_eq!(runner.call_count(), 1);
    }

    #[test]
    fn publish_exhausts_retries() {
        let runner = ScriptedRunner::new(vec![
            fail_output("failed to select a version"),
            fail_output("failed to select a version"),
        ]);
        let sleeper = RecordingSleeper::new();
        let config = RetryConfig {
            max_retries: 1,
            base: Duration::from_secs(5),
            factor: 2,
        };
        let attempt = publish_with_retries(
            &runner,
            &sleeper,
            Path::new("."),
            "relay",
            &PublishInvocation::default(),
            &config,
        )
        .expect("publish");
        assert!(!attempt.success);
        assert_eq!(attempt.retries, 1);
        assert_eq!(sleeper.sleeps(), vec![Duration::from_secs(5)]);
        assert_eq!(runner.call_count(), 2);
    }

    #[test]
    fn publish_args_shape() {
        let invocation = PublishInvocation {
            verify: false,
            allow_dirty: true,
            token: Some("secret".into()),
        };
        assert_eq!(
            publish_args("relay", &invocation),
            vec![
                "publish",
                "-p",
                "relay",
                "--no-verify",
                "--allow-dirty",
                "--token",
                "secret"
            ]
        );
        let verifying = PublishInvocation {
            verify: true,
            ..PublishInvocation::default()
        };
        assert_eq!(
            publish_args("relay", &verifying),
            vec!["publish", "-p", "relay"]
        );
    }

    #[test]
    fn package_list_args_shape() {
        assert_eq!(
            package_list_args("relay-core"),
            vec!["package", "--list", "-p", "relay-core", "--allow-dirty"]
        );
    }

    #[test]
    fn gate_commands_apply_exclude_to_build_test_clippy() {
        let commands = gate_commands(Some("relay-cli"));
        let names: Vec<&str> = commands.iter().map(|(name, _)| *name).collect();
        assert_eq!(names, vec!["build", "test", "clippy", "fmt"]);
        for (name, args) in &commands {
            let has_exclude = args.iter().any(|arg| arg == "--exclude");
            if *name == "fmt" {
                assert!(!has_exclude, "cargo fmt не поддерживает --exclude");
            } else {
                assert!(has_exclude, "{name} должен исключать запущенный крейт");
            }
            if *name == "clippy" {
                assert_eq!(args.last().map(String::as_str), Some("warnings"));
            }
        }
    }

    #[test]
    fn gate_commands_without_exclude_are_plain() {
        let commands = gate_commands(None);
        assert!(
            commands
                .iter()
                .all(|(_, args)| !args.iter().any(|arg| arg == "--exclude"))
        );
    }

    #[test]
    fn already_published_when_both_sources_agree() {
        let probe = MockProbe {
            outcome: ApiOutcome::Versions(vec![semver::Version::new(0, 2, 0)]),
        };
        let mut tags = BTreeSet::new();
        tags.insert("v0.2.0".to_string());
        let check =
            check_already_published(&probe, Some(&tags), "relay", &semver::Version::new(0, 2, 0));
        assert!(check.published);
        assert!(check.warnings.is_empty());
    }

    #[test]
    fn already_published_disagreement_warns_and_continues() {
        let probe = MockProbe {
            outcome: ApiOutcome::Versions(vec![semver::Version::new(0, 2, 0)]),
        };
        let tags = BTreeSet::new();
        let check =
            check_already_published(&probe, Some(&tags), "relay", &semver::Version::new(0, 2, 0));
        assert!(!check.published);
        assert_eq!(check.warnings.len(), 1);
        assert!(
            check.warnings[0].contains("расходятся"),
            "{:?}",
            check.warnings
        );
    }

    #[test]
    fn api_unreachable_falls_back_to_git_tags() {
        let probe = MockProbe {
            outcome: ApiOutcome::Unavailable,
        };
        let mut tags = BTreeSet::new();
        tags.insert("v0.2.0".to_string());
        let check =
            check_already_published(&probe, Some(&tags), "relay", &semver::Version::new(0, 2, 0));
        assert!(check.published);
        assert!(
            check.warnings[0].contains("falling back to git tags"),
            "{:?}",
            check.warnings
        );
    }

    #[test]
    fn not_found_on_registry_is_not_published() {
        let probe = MockProbe {
            outcome: ApiOutcome::NotFound,
        };
        let tags = BTreeSet::new();
        let check =
            check_already_published(&probe, Some(&tags), "relay", &semver::Version::new(0, 2, 0));
        assert!(!check.published);
        assert!(check.warnings.is_empty());
    }
}
