use std::collections::BTreeMap;

use serde_json::json;

use crate::manifest::Dependency;
use crate::resolve::RequiredComponent;
use crate::scan::ScanResult;

/// Проблема, обнаруженная при анализе проекта.
#[derive(Debug, Clone)]
pub struct Issue {
    /// Человекочитаемое описание проблемы.
    pub message: String,
}

/// Рекомендация добавить компонент.
#[derive(Debug, Clone)]
pub struct Recommendation {
    /// Имя крейта-компонента.
    pub name: String,
    /// Строка требования, например `light-fw-element-form ^0.3`.
    pub requirement: String,
}

/// Полный результат анализа проекта.
#[derive(Debug)]
pub struct ProjectReport {
    /// Результат сканирования шаблонов.
    pub scan: ScanResult,
    /// Установленные компоненты.
    pub installed: Vec<Dependency>,
    /// Используемые, но не установленные компоненты.
    pub missing: Vec<RequiredComponent>,
    /// Установленные, но не используемые компоненты.
    pub unused: Vec<Dependency>,
    /// Обнаруженные проблемы.
    pub issues: Vec<Issue>,
    /// Рекомендации по добавлению отсутствующих компонентов.
    pub recommendations: Vec<Recommendation>,
}

impl ProjectReport {
    /// Есть ли проблемы, из-за которых `check --strict` должен вернуть код 1.
    pub fn has_problems(&self) -> bool {
        !self.missing.is_empty() || !self.issues.is_empty() || !self.unused.is_empty()
    }

    /// Текстовый отчёт для терминала.
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        out.push_str("Отчёт о проекте Light\n\n");
        out.push_str(&format!("Шаблонов: {}\n", self.scan.files.len()));

        out.push_str("\nТеги:\n");
        if self.scan.tags.is_empty() {
            out.push_str("  (нет)\n");
        }
        for (name, count) in &self.scan.tags {
            out.push_str(&format!("  {name}: {count}\n"));
        }

        out.push_str("\nАтрибуты:\n");
        if self.scan.attributes.is_empty() {
            out.push_str("  (нет)\n");
        }
        for (name, count) in &self.scan.attributes {
            out.push_str(&format!("  {name}: {count}\n"));
        }

        out.push_str("\nУстановленные компоненты:\n");
        if self.installed.is_empty() {
            out.push_str("  (нет)\n");
        }
        for c in &self.installed {
            match &c.version {
                Some(v) => out.push_str(&format!("  {} {}\n", c.name, v)),
                None => out.push_str(&format!("  {}\n", c.name)),
            }
        }

        out.push_str("\nОтсутствующие компоненты:\n");
        if self.missing.is_empty() {
            out.push_str("  (нет)\n");
        }
        for c in &self.missing {
            out.push_str(&format!("  {}\n", c.name));
        }

        out.push_str("\nНеиспользуемые компоненты:\n");
        if self.unused.is_empty() {
            out.push_str("  (нет)\n");
        }
        for c in &self.unused {
            out.push_str(&format!("  {}\n", c.name));
        }

        if !self.issues.is_empty() {
            out.push_str("\nПроблемы:\n");
            for issue in &self.issues {
                out.push_str(&format!("  {}\n", issue.message));
            }
        }

        if !self.recommendations.is_empty() {
            out.push_str("\nРекомендации:\n");
            for r in &self.recommendations {
                out.push_str(&format!("  добавить {}\n", r.requirement));
            }
        }

        out
    }

    /// Отчёт в формате JSON (для `--format json`).
    pub fn to_json(&self) -> serde_json::Value {
        let mut tags: BTreeMap<String, usize> = BTreeMap::new();
        for (k, v) in &self.scan.tags {
            tags.insert(k.clone(), *v);
        }
        let mut attrs: BTreeMap<String, usize> = BTreeMap::new();
        for (k, v) in &self.scan.attributes {
            attrs.insert(k.clone(), *v);
        }
        json!({
            "files": self.scan.files.len(),
            "tags": tags,
            "attributes": attrs,
            "installed": self.installed.iter().map(|c| {
                json!({ "name": c.name, "version": c.version })
            }).collect::<Vec<_>>(),
            "missing": self.missing.iter().map(|c| c.name.clone()).collect::<Vec<_>>(),
            "unused": self.unused.iter().map(|c| c.name.clone()).collect::<Vec<_>>(),
            "issues": self.issues.iter().map(|i| i.message.clone()).collect::<Vec<_>>(),
            "recommendations": self.recommendations.iter().map(|r| {
                json!({ "name": r.name, "requirement": r.requirement })
            }).collect::<Vec<_>>(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::ScanResult;

    fn empty_report() -> ProjectReport {
        ProjectReport {
            scan: ScanResult::default(),
            installed: Vec::new(),
            missing: Vec::new(),
            unused: Vec::new(),
            issues: Vec::new(),
            recommendations: Vec::new(),
        }
    }

    #[test]
    fn empty_report_has_no_problems() {
        assert!(!empty_report().has_problems());
    }

    #[test]
    fn report_with_missing_component_has_problems() {
        let mut report = empty_report();
        report.missing.push(RequiredComponent {
            name: "light-fw-element-form".into(),
        });
        assert!(report.has_problems());
    }

    #[test]
    fn json_contains_expected_keys() {
        let value = empty_report().to_json();
        assert_eq!(value["files"], 0);
        assert!(value["tags"].is_object());
        assert!(value["missing"].is_array());
    }
}
