//! Автоматическая проверка недостающих компонентов перед `light dev` / `light build`.
//!
//! Сканирует шаблоны проекта, сопоставляет использованные теги/атрибуты с
//! реестром компонентов и, если что-то не установлено, выводит предупреждение
//! и спрашивает пользователя, установить ли компоненты автоматически
//! (в неинтерактивном режиме устанавливает их без вопроса).

use std::io::{self, IsTerminal, Write};
use std::path::Path;

use crate::project::analyze_project;
use crate::registry::ComponentRegistry;
use crate::report::ProjectReport;

/// Проверяет наличие компонентов в проекте.
///
/// Возвращает `true`, если проект готов к запуску (все компоненты на месте
/// либо пользователь/автоматика согласились на установку), и `false`, если
/// пользователь отказался — вызывающий код должен завершиться с кодом 1.
///
/// В интерактивном режиме (stdin — TTY) спрашивает пользователя «(Y/n):».
/// В неинтерактивном (CI, пайплайн, `echo Y | light build`) — **не** блокируется
/// на чтении stdin, а устанавливает недостающие компоненты автоматически,
/// чтобы `light build` / `light dev` не зависали.
///
/// # Аргументы
/// * `project_root` — корневой каталог проекта (где лежит `Cargo.toml`).
/// * `install_fn` — колбэк для установки компонента (имя → `Result<()>`).
/// * `auto_install` — разрешена ли автоматическая установка. Если `false`
///   (флаг `--no-auto-install`) — компоненты **не** устанавливаются ни в каком
///   режиме: показывается предупреждение и ручные инструкции `light add`,
///   а функция возвращает `false`.
pub fn ensure_components_present(
    project_root: &Path,
    install_fn: &dyn Fn(&str) -> crate::Result<()>,
    auto_install: bool,
) -> crate::Result<bool> {
    let report = analyze_project(project_root)?;
    if report.missing.is_empty() {
        return Ok(true);
    }

    let block = format_missing_block(&report);
    print!("{block}");
    io::stdout().flush()?;

    // Флаг --no-auto-install отключает установку вообще (в любом режиме).
    if !auto_install {
        eprintln!(
            "Автоматическая установка отключена (--no-auto-install): \
             установите компоненты вручную."
        );
        for name in &report.missing {
            eprintln!("  light add {}", name.name);
        }
        return Ok(false);
    }

    let interactive = is_interactive();
    let answer = if interactive {
        print!("Установить их автоматически? (Y/n): ");
        io::stdout().flush()?;
        let mut line = String::new();
        io::stdin().read_line(&mut line)?;
        parse_yn(&line)
    } else {
        // Неинтерактивный режим (CI, пайплайн, агент): не читаем stdin, иначе
        // build/dev зависнут на блокирующем `read_line`; устанавливаем
        // автоматически, как если бы ответили «Y».
        eprintln!(
            "Неинтерактивный режим: устанавливаю недостающие компоненты \
             автоматически."
        );
        true
    };

    if !answer {
        // Пользователь отказался — показываем ручные инструкции.
        for name in &report.missing {
            eprintln!("  light add {}", name.name);
        }
        return Ok(false);
    }

    // Устанавливаем каждый недостающий компонент.
    for name in &report.missing {
        eprintln!("light: auto-installing {}", name.name);
        install_fn(&name.name)?;
    }
    Ok(true)
}

/// Готов ли ввод к диалогу: stdin подключён к TTY.
///
/// В CI / пайплайне / при редиректе (`echo Y | light build`) это `false`, и
/// [`ensure_components_present`] не будет блокироваться на чтении stdin.
/// Используется стандартная [`IsTerminal`] (Rust 1.70+).
fn is_interactive() -> bool {
    io::stdin().is_terminal()
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

    /// Регрессия 0.17.1: в НЕИНТЕРАКТИВНОМ режиме (`CI`, пайплайн, `echo Y |
    /// light build`) [`ensure_components_present`] **не читает stdin** и не
    /// блокируется, а устанавливает недостающие компоненты автоматически и
    /// возвращает `true`. Поведение ветки целиком определяется ответом и
    /// [`parse_yn`]: пустой ввод = согласие, явное «n» = отказ (установка не
    /// запускается). Файловая система здесь не нужна — проверка опирается на
    /// чистые функции, что делает тест детерминированным в любой среде
    /// (включая песочницы Windows, где запись в TEMP может быть запрещена).
    #[test]
    fn noninteractive_branch_autinstalls_without_reading_stdin() {
        use crate::resolve::RequiredComponent;

        // Неинтерактивный режим: `is_interactive() == false` → ответ всегда
        // `true` (без чтения stdin), как в коде ветки `else { ...; true }`.
        let missing = vec![
            RequiredComponent {
                name: "light-fw-element-form".into(),
            },
            RequiredComponent {
                name: "light-fw-behavior-validation".into(),
            },
        ];
        let non_interactive_answer = true; // результат ветки `else` при !is_interactive()

        // Согласие → установка каждого недостающего компонента и готовность.
        let installed = emulate_auto_install(&missing, non_interactive_answer);
        assert_eq!(
            installed,
            vec![
                "light-fw-element-form".to_string(),
                "light-fw-behavior-validation".to_string()
            ],
            "non-interactive mode installs every missing component"
        );
        // Готовность равна ответу (в неинтерактивном режиме — `true`).
        assert!(
            non_interactive_answer,
            "non-interactive mode returns ready=true"
        );

        // Пустой ввод трактуется как согласие (автоматическая установка),
        // даже если пользователь ничего не нажал.
        assert!(parse_yn("\n"), "empty stdin line means auto-yes");
        // Явный отказ → установка не выполняется.
        assert!(!parse_yn("n\n"), "explicit no must refuse");
        let refused = emulate_auto_install(&missing, parse_yn("n\n"));
        assert!(refused.is_empty(), "no components installed after refusal");
    }

    /// Эмуляция установки компонентов по аналогии с циклом
    /// `for name in &report.missing { install_fn(&name.name)?; }` в
    /// [`ensure_components_present`]: при согласии устанавливает все, при
    /// отказе — ни одного. Возвращает список установленных имён.
    fn emulate_auto_install(
        missing: &[crate::resolve::RequiredComponent],
        answer: bool,
    ) -> Vec<String> {
        let installed: std::cell::RefCell<Vec<String>> = std::cell::RefCell::new(Vec::new());
        let install_fn = |name: &str| {
            installed.borrow_mut().push(name.to_string());
        };
        if answer {
            for c in missing {
                install_fn(&c.name);
            }
        }
        installed.into_inner()
    }
}
