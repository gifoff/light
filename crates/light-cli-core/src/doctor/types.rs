use serde::Serialize;

use super::checks::push_check;

/// Уровень серьёзности замечания.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Ошибка: ненулевой код возврата.
    Error,
    /// Предупреждение: не блокирует работу.
    Warning,
}

/// Замечание `light doctor`.
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
    /// Есть ли `light.lock`.
    pub lockfile_present: bool,
    /// Актуален ли `light.lock` (если он есть).
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

/// Полный отчёт `light doctor`.
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
        out.push_str("Light doctor\n");
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
        out.push_str(&format!("  · light {}\n", self.environment.cli));

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
                out.push_str("  ✓ light.lock актуален\n");
            } else {
                out.push_str("  ⚠ light.lock устарел (запустите `light lock`)\n");
            }
        } else {
            out.push_str("  ⚠ light.lock отсутствует (воспроизводимость не гарантирована)\n");
        }
        if let Some(core) = &self.project.core_version {
            out.push_str(&format!("  · light-fw-core {core}\n"));
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
