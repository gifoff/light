//! Команда `relay doctor`: диагностика окружения и проекта.
//!
//! Модуль только читает данные (файлы, версии инструментов, сеть) и ничего не
//! меняет на диске. Внешние зависимости (запуск `rustc`/`cargo`/`rustup` и
//! доступность сети) инжектируются через [`EnvironmentProbe`], поэтому логика
//! тестируется без реальных процессов.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::Result;
use crate::manifest::{
    has_package_section, read_cargo_toml, resolve_core_version, resolve_dependency_version,
};
use crate::process::{CommandRunner, SystemRunner, parse_program_version};
use crate::project::analyze_project;
use crate::registry::ComponentRegistry;

/// Минимальная поддерживаемая версия Rust.
pub const MIN_RUST_VERSION: (u64, u64, u64) = (1, 75, 0);

/// Источник данных об окружении.
pub trait EnvironmentProbe: Send + Sync {
    /// Версия `rustc`.
    fn rustc_version(&self) -> Option<semver::Version>;
    /// Версия `cargo`.
    fn cargo_version(&self) -> Option<semver::Version>;
    /// Версия `rustup` (опционально).
    fn rustup_version(&self) -> Option<semver::Version>;
    /// Доступна ли сеть (crates.io).
    fn network_reachable(&self) -> bool;
}

/// Реальный опрос окружения через запуск процессов.
pub struct SystemProbe {
    runner: SystemRunner,
    assume_offline: bool,
}

impl Default for SystemProbe {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemProbe {
    /// Создаёт пробник на базе системного раннера.
    pub fn new() -> Self {
        Self {
            runner: SystemRunner,
            assume_offline: false,
        }
    }

    /// Пробник, который не проверяет сеть (режим `--offline`).
    pub fn offline() -> Self {
        Self {
            runner: SystemRunner,
            assume_offline: true,
        }
    }

    fn probe_version(&self, program: &str) -> Option<semver::Version> {
        let output = self
            .runner
            .run(program, &["--version".to_string()], None)
            .ok()?;
        if !output.success() {
            return None;
        }
        parse_program_version(&output.stdout)
    }
}

impl EnvironmentProbe for SystemProbe {
    fn rustc_version(&self) -> Option<semver::Version> {
        self.probe_version("rustc")
    }

    fn cargo_version(&self) -> Option<semver::Version> {
        self.probe_version("cargo")
    }

    fn rustup_version(&self) -> Option<semver::Version> {
        self.probe_version("rustup")
    }

    fn network_reachable(&self) -> bool {
        if self.assume_offline {
            return false;
        }
        #[cfg(feature = "network")]
        {
            crate::fetch::ping_crates_io()
        }
        #[cfg(not(feature = "network"))]
        {
            false
        }
    }
}

/// Уровень серьёзности замечания.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Ошибка: ненулевой код возврата.
    Error,
    /// Предупреждение: не блокирует работу.
    Warning,
}

/// Замечание `relay doctor`.
#[derive(Debug, Clone, Serialize)]
pub struct DoctorIssue {
    /// Уровень.
    pub severity: Severity,
    /// Описание.
    pub message: String,
}

/// Данные об окружении.
#[derive(Debug, Clone, Serialize)]
pub struct EnvironmentSection {
    /// Версия `rustc` (`null`, если не найден).
    pub rustc: Option<String>,
    /// Версия `cargo`.
    pub cargo: Option<String>,
    /// Версия `rustup` (опционально).
    pub rustup: Option<String>,
    /// Доступна ли сеть.
    pub network: bool,
    /// Версия самого CLI.
    pub cli: String,
}

/// Данные о проекте.
#[derive(Debug, Clone, Serialize)]
pub struct ProjectSection {
    /// Найден ли `Cargo.toml`.
    pub cargo_toml: bool,
    /// Есть ли секция `[package]`.
    pub package_section: bool,
    /// Найдено ли шаблонов.
    pub templates_found: usize,
    /// Есть ли `relay.lock`.
    pub lockfile_present: bool,
    /// Актуален ли `relay.lock` (если он есть).
    pub lockfile_actual: bool,
    /// Версия ядра проекта (если определена).
    pub core_version: Option<String>,
}

/// Статус одного установленного компонента.
#[derive(Debug, Clone, Serialize)]
pub struct ComponentStatus {
    /// Имя крейта.
    pub name: String,
    /// Установленная версия.
    pub version: Option<String>,
    /// Совместим ли с ядром.
    pub compatible: bool,
}

/// Полный отчёт `relay doctor`.
#[derive(Debug, Clone, Serialize)]
pub struct DoctorReport {
    /// Окружение.
    pub environment: EnvironmentSection,
    /// Проект.
    pub project: ProjectSection,
    /// Компоненты.
    pub components: Vec<ComponentStatus>,
    /// Замечания.
    pub issues: Vec<DoctorIssue>,
}

impl DoctorReport {
    /// Есть ли ошибки (ненулевой код возврата).
    pub fn has_errors(&self) -> bool {
        self.issues.iter().any(|i| i.severity == Severity::Error)
    }

    /// Текстовый отчёт.
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        out.push_str("Relay doctor\n");
        out.push_str("────────────────────────────────────────\n\n");

        out.push_str("Окружение:\n");
        push_check(&mut out, self.environment.rustc.as_deref(), "rustc", true);
        push_check(&mut out, self.environment.cargo.as_deref(), "cargo", true);
        match &self.environment.rustup {
            Some(v) => out.push_str(&format!("  ✓ rustup {v}\n")),
            None => out.push_str("  ⚠ rustup не обнаружен (не обязательно)\n"),
        }
        if self.environment.network {
            out.push_str("  ✓ сеть crates.io доступна\n");
        } else {
            out.push_str("  ⚠ сеть crates.io недоступна (offline)\n");
        }
        out.push_str(&format!("  · relay {}\n", self.environment.cli));

        out.push_str("\nПроект:\n");
        if self.project.cargo_toml {
            out.push_str("  ✓ Cargo.toml найден\n");
        } else {
            out.push_str("  ✗ Cargo.toml не найден\n");
        }
        out.push_str(&format!(
            "  {} templates/ — {} файлов\n",
            if self.project.templates_found > 0 {
                "✓"
            } else {
                "⚠"
            },
            self.project.templates_found
        ));
        if self.project.lockfile_present {
            if self.project.lockfile_actual {
                out.push_str("  ✓ relay.lock актуален\n");
            } else {
                out.push_str("  ⚠ relay.lock устарел (запустите `relay lock`)\n");
            }
        } else {
            out.push_str("  ⚠ relay.lock отсутствует (воспроизводимость не гарантирована)\n");
        }
        if let Some(core) = &self.project.core_version {
            out.push_str(&format!("  · relay-core {core}\n"));
        }

        out.push_str(&format!("\nКомпоненты ({}):\n", self.components.len()));
        if self.components.is_empty() {
            out.push_str("  (нет)\n");
        }
        for c in &self.components {
            let mark = if c.compatible { "✓" } else { "✗" };
            let version = c.version.as_deref().unwrap_or("?");
            let compat = if c.compatible {
                "совместим"
            } else {
                "несовместим"
            };
            out.push_str(&format!("  {mark} {} {version} ({compat})\n", c.name));
        }

        out.push_str("\nЗамечания:\n");
        if self.issues.is_empty() {
            out.push_str("  нет\n");
        }
        for issue in &self.issues {
            let mark = match issue.severity {
                Severity::Error => "✗",
                Severity::Warning => "⚠",
            };
            out.push_str(&format!("  {mark} {}\n", issue.message));
        }

        out.push_str("\n────────────────────────────────────────\n");
        if self.has_errors() {
            out.push_str("Итог: обнаружены проблемы.\n");
        } else {
            out.push_str("Итог: всё в порядке.\n");
        }
        out
    }

    /// Отчёт в формате JSON (совпадает со структурой [`DoctorReport`]).
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

fn push_check(out: &mut String, version: Option<&str>, program: &str, required: bool) {
    match version {
        Some(v) => out.push_str(&format!("  ✓ {program} {v}\n")),
        None if required => out.push_str(&format!("  ✗ {program} не найден\n")),
        None => out.push_str(&format!("  ⚠ {program} не найден (не обязательно)\n")),
    }
}

/// Запускает диагностику проекта `project_root`.
pub fn doctor(project_root: &Path, probe: &dyn EnvironmentProbe) -> Result<DoctorReport> {
    let mut issues = Vec::new();

    let environment = check_environment(probe, &mut issues);
    let (project, components) = check_project(project_root, &mut issues)?;

    Ok(DoctorReport {
        environment,
        project,
        components,
        issues,
    })
}

fn check_environment(
    probe: &dyn EnvironmentProbe,
    issues: &mut Vec<DoctorIssue>,
) -> EnvironmentSection {
    let rustc = probe.rustc_version();
    let cargo = probe.cargo_version();
    let rustup = probe.rustup_version();
    let network = probe.network_reachable();

    match &rustc {
        Some(v) => {
            let min =
                semver::Version::new(MIN_RUST_VERSION.0, MIN_RUST_VERSION.1, MIN_RUST_VERSION.2);
            if *v < min {
                issues.push(DoctorIssue {
                    severity: Severity::Error,
                    message: format!(
                        "rustc {v} старше требуемой {}.{}.{}",
                        MIN_RUST_VERSION.0, MIN_RUST_VERSION.1, MIN_RUST_VERSION.2
                    ),
                });
            }
        }
        None => issues.push(DoctorIssue {
            severity: Severity::Error,
            message: "rustc не найден в PATH".to_string(),
        }),
    }
    if cargo.is_none() {
        issues.push(DoctorIssue {
            severity: Severity::Error,
            message: "cargo не найден в PATH".to_string(),
        });
    }
    if rustup.is_none() {
        issues.push(DoctorIssue {
            severity: Severity::Warning,
            message: "rustup не обнаружен (не обязательно)".to_string(),
        });
    }
    if !network {
        issues.push(DoctorIssue {
            severity: Severity::Warning,
            message: "сеть crates.io недоступна — доступен только vendor/workspace".to_string(),
        });
    }

    EnvironmentSection {
        rustc: rustc.map(|v| v.to_string()),
        cargo: cargo.map(|v| v.to_string()),
        rustup: rustup.map(|v| v.to_string()),
        network,
        cli: env!("CARGO_PKG_VERSION").to_string(),
    }
}

fn check_project(
    project_root: &Path,
    issues: &mut Vec<DoctorIssue>,
) -> Result<(ProjectSection, Vec<ComponentStatus>)> {
    let cargo_path = project_root.join("Cargo.toml");
    let cargo_toml = cargo_path.is_file();
    let mut package_section = false;
    if !cargo_toml {
        issues.push(DoctorIssue {
            severity: Severity::Error,
            message: "Cargo.toml не найден в указанном каталоге".to_string(),
        });
    } else {
        let text = read_cargo_toml(&cargo_path)?;
        package_section = has_package_section(&text)?;
        if !package_section {
            issues.push(DoctorIssue {
                severity: Severity::Warning,
                message: "в Cargo.toml нет секции [package]".to_string(),
            });
        }
    }

    let (installed, templates_found) = if cargo_toml {
        let report = analyze_project(project_root)?;
        (report.installed, report.scan.files.len())
    } else {
        (
            Vec::new(),
            crate::scan::scan_templates(project_root)?.files.len(),
        )
    };
    if templates_found == 0 {
        issues.push(DoctorIssue {
            severity: Severity::Warning,
            message: "не найдено ни одного *.html-шаблона (templates/)".to_string(),
        });
    }

    let core_version = if cargo_toml {
        resolve_core_version(project_root)?
    } else {
        None
    };
    if core_version.is_none() && cargo_toml {
        issues.push(DoctorIssue {
            severity: Severity::Warning,
            message: "не удалось определить версию relay-core".to_string(),
        });
    }

    let registry = ComponentRegistry::builtin();
    let mut components = Vec::new();
    for dep in &installed {
        let info = registry.by_name(&dep.name);
        let dep_version = resolve_dependency_version(project_root, &dep.name)?
            .or_else(|| info.map(|i| i.latest_version.clone()));
        let compatible = match (info, &core_version) {
            (Some(info), Some(core)) => info.core_requirement.matches(core),
            _ => true,
        };
        if !compatible {
            let reason = info
                .map(|i| i.core_requirement.to_string())
                .unwrap_or_default();
            issues.push(DoctorIssue {
                severity: Severity::Error,
                message: format!(
                    "{} несовместим с relay-core {} (требуется {})",
                    dep.name,
                    core_version
                        .as_ref()
                        .map(|v| v.to_string())
                        .unwrap_or_default(),
                    reason
                ),
            });
        }
        components.push(ComponentStatus {
            name: dep.name.clone(),
            version: dep_version.map(|v| v.to_string()),
            compatible,
        });
    }

    let lock_path = project_root.join(crate::lockfile::LOCKFILE_NAME);
    let lockfile_present = lock_path.is_file();
    let lockfile_actual = if lockfile_present && cargo_toml {
        crate::lockfile::is_actual(project_root)?
    } else {
        false
    };

    let project = ProjectSection {
        cargo_toml,
        package_section,
        templates_found,
        lockfile_present,
        lockfile_actual,
        core_version: core_version.map(|v| v.to_string()),
    };
    Ok((project, components))
}

/// Каталог проекта по умолчанию.
pub fn default_project_dir() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    struct MockProbe {
        rustc: Option<semver::Version>,
        cargo: Option<semver::Version>,
        rustup: Option<semver::Version>,
        network: bool,
    }

    impl EnvironmentProbe for MockProbe {
        fn rustc_version(&self) -> Option<semver::Version> {
            self.rustc.clone()
        }
        fn cargo_version(&self) -> Option<semver::Version> {
            self.cargo.clone()
        }
        fn rustup_version(&self) -> Option<semver::Version> {
            self.rustup.clone()
        }
        fn network_reachable(&self) -> bool {
            self.network
        }
    }

    fn ok_probe() -> MockProbe {
        MockProbe {
            rustc: Some(semver::Version::new(1, 80, 0)),
            cargo: Some(semver::Version::new(1, 80, 0)),
            rustup: Some(semver::Version::new(1, 27, 0)),
            network: true,
        }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("relay-cli-doctor-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn write_project(dir: &Path, cargo: &str, templates: &[(&str, &str)]) {
        fs::write(dir.join("Cargo.toml"), cargo).expect("write Cargo.toml");
        if !templates.is_empty() {
            let tdir = dir.join("templates");
            fs::create_dir_all(&tdir).expect("create templates");
            for (name, content) in templates {
                fs::write(tdir.join(name), content).expect("write template");
            }
        }
    }

    #[test]
    fn detects_core_version_in_workspace() {
        let root = temp_dir("workspace-core");
        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"app\", \"relay\"]\nresolver = \"2\"\n\n\
             [workspace.package]\nversion = \"0.2.3\"\n\n\
             [workspace.dependencies]\nrelay = { path = \"relay\" }\n",
        )
        .expect("root manifest");
        let app = root.join("app");
        fs::create_dir_all(&app).expect("app");
        fs::write(
            app.join("Cargo.toml"),
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
             [dependencies]\nrelay = { workspace = true }\n",
        )
        .expect("app manifest");
        let relay = root.join("relay");
        fs::create_dir_all(relay.join("src")).expect("relay src");
        fs::write(relay.join("src/lib.rs"), "").expect("relay lib");
        fs::write(
            relay.join("Cargo.toml"),
            "[package]\nname = \"relay\"\nversion.workspace = true\nedition = \"2021\"\n",
        )
        .expect("relay manifest");

        let report = doctor(&app, &ok_probe()).expect("doctor");
        assert_eq!(report.project.core_version.as_deref(), Some("0.2.3"));
        assert!(
            !report.issues.iter().any(|i| i
                .message
                .contains("не удалось определить версию relay-core")),
            "ядро определено: {:?}",
            report.issues
        );
    }

    #[test]
    fn healthy_project_has_no_errors() {
        let dir = temp_dir("healthy");
        write_project(
            &dir,
            "[package]\nname = \"p\"\nversion = \"0.1.0\"\n[workspace]\n\n[dependencies]\nrelay = \"0.1\"\n",
            &[("app.html", "<div></div>")],
        );
        let report = doctor(&dir, &ok_probe()).expect("doctor");
        assert!(!report.has_errors(), "issues: {:?}", report.issues);
        assert_eq!(report.project.templates_found, 1);
        assert!(!report.project.lockfile_present);
        assert!(report.components.is_empty());
    }

    #[test]
    fn old_rustc_is_an_error() {
        let dir = temp_dir("old-rustc");
        write_project(
            &dir,
            "[package]\nname = \"p\"\nversion = \"0.1.0\"\n[workspace]\n\n[dependencies]\nrelay = \"0.1\"\n",
            &[("app.html", "<div></div>")],
        );
        let mut probe = ok_probe();
        probe.rustc = Some(semver::Version::new(1, 60, 0));
        let report = doctor(&dir, &probe).expect("doctor");
        assert!(report.has_errors());
    }

    #[test]
    fn missing_templates_is_warning_not_error() {
        let dir = temp_dir("no-templates");
        write_project(
            &dir,
            "[package]\nname = \"p\"\nversion = \"0.1.0\"\n[workspace]\n\n[dependencies]\nrelay = \"0.1\"\n",
            &[],
        );
        let report = doctor(&dir, &ok_probe()).expect("doctor");
        assert!(!report.has_errors());
        assert_eq!(report.project.templates_found, 0);
        assert!(
            report
                .issues
                .iter()
                .any(|i| i.severity == Severity::Warning && i.message.contains("шаблон"))
        );
    }

    #[test]
    fn incompatible_component_is_error() {
        let dir = temp_dir("incompatible");
        write_project(
            &dir,
            "[package]\nname = \"p\"\nversion = \"0.1.0\"\n[workspace]\n\n[dependencies]\nrelay = \"0.1\"\nrelay-element-form = \"0.1\"\n",
            &[("app.html", "<checkbox></checkbox>")],
        );
        // Registry требует core ^0.2; проект на relay 0.1 — несовместимо.
        let report = doctor(&dir, &ok_probe()).expect("doctor");
        assert!(
            report
                .components
                .iter()
                .any(|c| c.name == "relay-element-form")
        );
        assert!(report.has_errors(), "issues: {:?}", report.issues);
    }

    #[test]
    fn missing_cargo_toml_is_error() {
        let dir = temp_dir("no-manifest");
        let report = doctor(&dir, &ok_probe()).expect("doctor");
        assert!(report.has_errors());
    }

    #[test]
    fn json_serializes_and_round_trips() {
        let dir = temp_dir("json");
        write_project(
            &dir,
            "[package]\nname = \"p\"\nversion = \"0.1.0\"\n[workspace]\n\n[dependencies]\nrelay = \"0.1\"\n",
            &[("app.html", "<div></div>")],
        );
        let report = doctor(&dir, &ok_probe()).expect("doctor");
        let value = report.to_json();
        assert_eq!(value["project"]["templates_found"], 1);
        assert_eq!(value["environment"]["cli"], env!("CARGO_PKG_VERSION"));
        assert!(value["issues"].is_array());
    }
}
