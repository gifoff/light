//! Автоматическая проверка недостающих компонентов перед `light dev` / `light build`.
//!
//! Сканирует шаблоны проекта, сопоставляет использованные теги/атрибуты с
//! реестром компонентов и, если что-то не установлено, выводит предупреждение
//! и спрашивает пользователя,要不要 установить компоненты автоматически.

use std::io::{self, BufRead, Write};
use std::path::Path;

use crate::project::analyze_project;
use crate::registry::ComponentRegistry;
use crate::report::ProjectReport;

/// Проверяет наличие компонентов в проекте.
///
/// Возвращает `true`, если проект готов к запуску (все компоненты на месте
/// либо пользователь согласился на автоматическую установку), и `false`, если
/// пользователь отказался от установки — вызывающий код должен завершиться
/// с кодом 1.
///
/// # Аргументы
/// * `project_root` — корневой каталог проекта (где лежит `Cargo.toml`).
/// * `install_fn` — колбэк для установки компонента (имя → `Result<()>`).
pub fn ensure_components_present(
    project_root: &Path,
    install_fn: &dyn Fn(&str) -> crate::Result<()>,
) -> crate::Result<bool> {
    let report = analyze_project(project_root)?;
    if report.missing.is_empty() {
        return Ok(true);
    }

    let block = format_missing_block(&report);
    print!("{block}");
    io::stdout().flush()?;

    print!("Установить их автоматически? (Y/n): ");
    io::stdout().flush()?;

    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    let answer = parse_yn(&line);

    if !answer {
        // Пользователь отказался — показываем ручные инструкции.
        for name in &report.missing {
            eprintln!("  light add {}", name.name);
        }
        return Ok(false);
    }

    // Устанавливаем каждый недостающий компонент.
    for name in &report.missing {
        eprintln!("Устанавливаю {} ...", name.name);
        install_fn(&name.name)?;
    }
    Ok(true)
}

/// Форматирует блок «⚠️ Обнаружены недостающие компоненты» по спецификации ТЗ.
///
/// Для каждого недостающего компонента ищет первый тег из его `provides.elements`,
/// который встречается в сканированных шаблонах, и добавляет `(для <tag>)`.
pub fn format_missing_block(report: &ProjectReport) -> String {
    let registry = ComponentRegistry::builtin();
    let mut out = String::from("⚠️ Обнаружены недостающие компоненты:\n");
    for comp in &report.missing {
        let info = registry.by_name(&comp.name);
        let tag_hint = info
            .and_then(|c| {
                c.provides
                    .elements
                    .iter()
                    .find(|el| report.scan.tags.contains_key(*el))
            })
            .map(|el| format!(" (для <{el}>)"))
            .unwrap_or_default();
        out.push_str(&format!("   - {}{}\n", comp.name, tag_hint));
    }
    out
}

/// Разбирает ответ пользователя: пустая строка / "y" / "yes" (без учёта
/// регистра) → `true`; иначе → `false`.
pub fn parse_yn(line: &str) -> bool {
    let trimmed = line.trim().to_lowercase();
    matches!(trimmed.as_str(), "" | "y" | "yes")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::Dependency;
    use crate::report::{Issue, ProjectReport, Recommendation};
    use crate::resolve::RequiredComponent;
    use crate::scan::ScanResult;

    fn test_report() -> ProjectReport {
        let mut scan = ScanResult::default();
        scan.tags.insert("select".to_string(), 1);
        scan.attributes.insert("required".to_string(), 2);

        ProjectReport {
            scan,
            installed: Vec::<Dependency>::new(),
            missing: vec![
                RequiredComponent {
                    name: "light-fw-element-form".into(),
                },
                RequiredComponent {
                    name: "light-fw-behavior-validation".into(),
                },
            ],
            unused: Vec::new(),
            issues: Vec::<Issue>::new(),
            recommendations: Vec::<Recommendation>::new(),
        }
    }

    #[test]
    fn format_missing_block_with_tags() {
        let report = test_report();
        let block = format_missing_block(&report);
        assert!(block.starts_with("⚠️ Обнаружены недостающие компоненты:"));
        assert!(block.contains("- light-fw-element-form"));
        assert!(block.contains("- light-fw-behavior-validation"));
    }

    #[test]
    fn parse_yn_accepts_yes() {
        assert!(parse_yn("y"));
        assert!(parse_yn("Y"));
        assert!(parse_yn("yes"));
        assert!(parse_yn("YES"));
        assert!(parse_yn("Yes"));
    }

    #[test]
    fn parse_yn_accepts_empty() {
        assert!(parse_yn(""));
        assert!(parse_yn("  \n"));
    }

    #[test]
    fn parse_yn_rejects_no() {
        assert!(!parse_yn("n"));
        assert!(!parse_yn("no"));
        assert!(!parse_yn("N"));
        assert!(!parse_yn("hello"));
    }
}
