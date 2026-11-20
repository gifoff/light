use std::collections::BTreeMap;
use std::io::{self, Write};
use std::path::Path;

use super::helpers::{attr_used, extract_tag_names, is_interactive, is_standard_tag, meta_to_info};
use super::types::{AvailableComponent, CheckReport, ComponentInfo, Conflict};
use crate::metadata::load_installed;
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
        eprintln!("⏳ Установка {}…", name.name);
        eprintln!("  → cargo check…");
        install_fn(&name.name)?;
        eprintln!("  ✅ Компонент установлен.");
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

// ─────────────────────────── Умная проверка (Этап 1 Plugin API) ────────────────
//
// Три категории использованных тегов:
//   * **установлено** — тег предоставляется установленным компонентом;
//   * **доступно**    — тег не установлен, но есть во встроенном реестре
//                       (можно подключить через `light add`);
//   * **неизвестно**  — тег не из стандартного HTML и его нет в реестре
//                       (возможно, нужен плагин — см. `light search`).

/// Извлекает имена использованных тегов из всех `templates/**/*.html`.
///
/// Простой regex-разбор: после каждой `<` читаем до первого пробела/`>`/`/>`
/// имя, начинающееся с буквы. Не разбирает вложенность и комментарии —
/// достаточно для сопоставления с реестром (это не парсер шаблонов).
pub fn scan_used_tags(project_root: &Path) -> crate::Result<BTreeMap<String, usize>> {
    let templates_dir = project_root.join("templates");
    let mut tags: BTreeMap<String, usize> = BTreeMap::new();
    if !templates_dir.exists() {
        return Ok(tags);
    }
    let walker = ignore::WalkBuilder::new(&templates_dir)
        .hidden(true)
        .filter_entry(|e| e.file_name() != "target")
        .build();
    for entry in walker {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("html") {
            continue;
        }
        let text = std::fs::read_to_string(path)?;
        extract_tag_names(&text, &mut tags);
    }
    Ok(tags)
}

/// Чистое сопоставление использованных тегов с установленными компонентами.
/// Используется как ядро [`build_check_report`] и для unit-тестов.
pub fn compare_tags(used: &BTreeMap<String, usize>, installed: &[ComponentInfo]) -> CheckReport {
    let registry = ComponentRegistry::builtin();

    // Тег → обладатели (установленные компоненты, предоставляющие этот тег).
    let mut tag_owners: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for comp in installed {
        for tag in &comp.provides.elements {
            tag_owners
                .entry(tag.clone())
                .or_default()
                .push(comp.crate_name.clone());
        }
    }
    let installed_tags: std::collections::HashSet<&str> =
        tag_owners.keys().map(String::as_str).collect();

    // Классификация использованных тегов.
    let mut available: Vec<AvailableComponent> = Vec::new();
    let mut unknown: Vec<String> = Vec::new();
    for tag in used.keys() {
        if is_standard_tag(tag) {
            continue;
        }
        if installed_tags.contains(tag.as_str()) {
            continue;
        }
        // Недостает: ищем в реестре.
        if let Some(info) = registry.component_for_element(tag) {
            available.push(AvailableComponent {
                tag: tag.clone(),
                crate_name: info.name.clone(),
                requirement: info.requirement(),
            });
        } else {
            unknown.push(tag.clone());
        }
    }
    // Дедупликация: если несколько тегов ведут к одному крейту — один раз.
    available.sort_by(|a, b| a.crate_name.cmp(&b.crate_name));
    available.dedup_by(|a, b| a.crate_name == b.crate_name);

    // Неиспользуемые: установлены, но их элементы/атрибуты/директивы не
    // встречены в шаблонах (атрибуты могут иметь префиксы `:`, `@`, `v-`).
    let unused: Vec<ComponentInfo> = installed
        .iter()
        .filter(|c| {
            !c.provides.elements.iter().any(|t| used.contains_key(t))
                && !c.provides.attributes.iter().any(|a| attr_used(used, a))
                && !c.provides.directives.iter().any(|d| attr_used(used, d))
        })
        .cloned()
        .collect();

    // Конфликты: тег, у которого ≥ 2 обладателей.
    let mut conflicts: Vec<Conflict> = Vec::new();
    for (tag, mut owners) in tag_owners {
        if owners.len() >= 2 {
            owners.sort();
            owners.dedup();
            if owners.len() >= 2 {
                conflicts.push(Conflict { tag, owners });
            }
        }
    }
    conflicts.sort_by(|a, b| a.tag.cmp(&b.tag));

    CheckReport {
        installed: installed.to_vec(),
        available,
        unknown,
        unused,
        conflicts,
    }
}

/// Строит [`CheckReport`] по проекту: сканирует теги, читает установленные
/// компоненты через `cargo metadata`, сопоставляет со встроенным реестром.
pub fn build_check_report(project_root: &Path) -> crate::Result<CheckReport> {
    let used = scan_used_tags(project_root)?;
    let installed_raw = load_installed(project_root)?;
    let installed: Vec<ComponentInfo> = installed_raw.into_iter().map(meta_to_info).collect();
    Ok(compare_tags(&used, &installed))
}
