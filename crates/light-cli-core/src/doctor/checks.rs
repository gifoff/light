use std::path::{Path, PathBuf};

use crate::manifest::{
    has_package_section, read_cargo_toml, resolve_core_version, resolve_dependency_version,
};
use crate::project::analyze_project;
use crate::registry::ComponentRegistry;

use super::probe::{EnvironmentProbe, MIN_RUST_VERSION};
use super::types::{
    ComponentStatus, DoctorIssue, DoctorReport, EnvironmentSection, ProjectSection, Severity,
};

pub(crate) fn push_check(out: &mut String, version: Option<&str>, program: &str, required: bool) {
    match version {
        Some(v) => out.push_str(&format!("  ✓ {program} {v}\n")),
        None if required => out.push_str(&format!("  ✗ {program} не найден\n")),
        None => out.push_str(&format!("  ⚠ {program} не найден (не обязательно)\n")),
    }
}

/// Запускает диагностику проекта `project_root`.
pub fn doctor(
    project_root: &Path,
    probe: &dyn EnvironmentProbe,
) -> std::result::Result<DoctorReport, crate::error::Error> {
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
) -> std::result::Result<(ProjectSection, Vec<ComponentStatus>), crate::error::Error> {
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
            message: "не удалось определить версию light-fw-core".to_string(),
        });
    }

    let registry = ComponentRegistry::builtin();
    let mut components = Vec::new();
    for dep in &installed {
        let info = registry.by_name(&dep.name);
        let dep_version = resolve_dependency_version(project_root, &dep.name)?
            .or_else(|| info.map(|i| i.latest_version.clone()));
        // Единое правило pre-release (FIX7, FIX8b): ядро 0.21.0-alpha.1
        // считается 0.21.0 при проверке совместимости. Срезаем pre-release и
        // build-метаданные у ТЕКУЩЕЙ версии ядра перед матчем (не у требования),
        // как в `crate::metadata::installed::check_core` — иначе формальный
        // semver 2.0 не матчит `^0.21` против `0.21.0-alpha.1` и doctor
        // ложно помечает компонент «несовместим».
        let mut effective_core = core_version.clone();
        if let Some(v) = effective_core.as_mut() {
            v.pre = semver::Prerelease::EMPTY;
            v.build = semver::BuildMetadata::EMPTY;
        }
        let compatible = match (info, &effective_core) {
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
                    "{} несовместим с light-fw-core {} (требуется {})",
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
