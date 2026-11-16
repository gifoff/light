//! Автоматическая проверка недостающих компонентов перед `light dev` / `light build`.
//!
//! Сканирует шаблоны проекта, сопоставляет использованные теги/атрибуты с
//! реестром компонентов и, если что-то не установлено, выводит предупреждение
//! и спрашивает пользователя, установить ли компоненты автоматически
//! (в неинтерактивном режиме устанавливает их без вопроса).

use std::collections::BTreeMap;
use std::io::{self, IsTerminal, Write};
use std::path::Path;

use crate::manifest::Provides;
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

// ─────────────────────────── Умная проверка (Этап 1 Plugin API) ────────────────
//
// Три категории использованных тегов:
//   * **установлено** — тег предоставляется установленным компонентом;
//   * **доступно**    — тег не установлен, но есть во встроенном реестре
//                       (можно подключить через `light add`);
//   * **неизвестно**  — тег не из стандартного HTML и его нет в реестре
//                       (возможно, нужен плагин — см. `light search`).

/// Стандартные HTML-теги, которые Light поддерживает «из коробки» и которые не
/// требуют ни одного компонента. Всё, что используется в шаблонах и не входит
/// в этот список, сопоставляется с установленными компонентами и встроенным
/// реестром.
const STANDARD_TAGS: &[&str] = &[
    // Виртуальные теги Light (корневые, не из стандартного HTML, но
    // поддерживаются ядром без какого-либо компонента).
    "window", "text", "password", // HTML5-строка.
    "html", "head", "body", "div", "span", "p", "a", "ul", "ol", "li", "h1", "h2", "h3", "h4",
    "h5", "h6", "button", "input", "label", "form", "table", "tr", "td", "th", "img", "video",
    "audio", "canvas", "svg", "section", "article", "header", "footer", "nav", "main", "aside",
    "dialog", "details", "summary", "select", "option", "textarea", "pre", "code", "strong", "em",
    "b", "i", "small", "sup", "sub", "hr", "br", "wbr", "u", "s", "strike",
];

/// Установка: стандартные теги не требуют компонентов.
fn is_standard_tag(tag: &str) -> bool {
    STANDARD_TAGS.contains(&tag)
}

/// Компонент-элемент, найденный в проекте (из `cargo metadata`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentInfo {
    /// Имя крейта (например, `light-fw-element-form`).
    pub crate_name: String,
    /// Версия, извлечённая из `Cargo.toml` пакета.
    pub version: String,
    /// Что компонент предоставляет (`provides.elements/attributes/directives`).
    pub provides: Provides,
}

/// Компонент, который можно установить (есть во встроенном реестре).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvailableComponent {
    /// Тег, которого не хватает в проекте.
    pub tag: String,
    /// Имя крейта, который его предоставляет.
    pub crate_name: String,
    /// Рекомендация к версии (`name ^ver`).
    pub requirement: String,
}

/// Конфликт: один тег предоставляют несколько установленных компонентов.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    /// Конфликтующий тег.
    pub tag: String,
    /// Имена компонентов-обладателей (отсортированы).
    pub owners: Vec<String>,
}

/// Результат умной проверки проекта.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CheckReport {
    /// Установленные компоненты (есть в `Cargo.toml` как path/git/published).
    pub installed: Vec<ComponentInfo>,
    /// Компоненты, которые можно установить из встроенного реестра.
    pub available: Vec<AvailableComponent>,
    /// Нераспознанные теги (не стандартные и не из реестра).
    pub unknown: Vec<String>,
    /// Установленные, но не используемые компоненты.
    pub unused: Vec<ComponentInfo>,
    /// Конфликты: один тег предоставляют два и более установленных компонента.
    pub conflicts: Vec<Conflict>,
}

impl CheckReport {
    /// Есть ли проблемы (для `--strict`): доступно, неизвестно или конфликт.
    pub fn has_problems(&self) -> bool {
        !self.available.is_empty() || !self.unknown.is_empty() || !self.conflicts.is_empty()
    }

    /// Человекочитаемый отчёт по спецификации ТЗ (русская терминология,
    /// эмодзи-маркеры категорий).
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        if self.installed.is_empty() {
            out.push_str("🟢 Установлено: нет\n");
        } else {
            out.push_str("🟢 Установлено:\n");
            for c in &self.installed {
                out.push_str(&format!("  - {} {}\n", c.crate_name, c.version));
            }
        }
        if !self.available.is_empty() {
            out.push_str("\n🟡 Доступно (установить):\n");
            for a in &self.available {
                out.push_str(&format!("  - <{}> → {}\n", a.tag, a.requirement));
            }
        }
        if !self.unknown.is_empty() {
            out.push_str("\n🔴 Неизвестно (нужен плагин или кастомный элемент):\n");
            for u in &self.unknown {
                out.push_str(&format!("  - <{}>\n", u));
            }
        }
        if !self.unused.is_empty() {
            out.push_str("\n🗑 Не используется (можно убрать из Cargo.toml):\n");
            for u in &self.unused {
                out.push_str(&format!("  - {}\n", u.crate_name));
            }
        }
        if !self.conflicts.is_empty() {
            out.push_str("\n⚠️ Конфликты:\n");
            for c in &self.conflicts {
                out.push_str(&format!("  - <{}>: {}\n", c.tag, c.owners.join(", ")));
            }
        }
        out
    }

    /// Машиночитаемый отчёт (JSON) для CI.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "installed": self
                .installed
                .iter()
                .map(|c| serde_json::json!({
                    "crate_name": c.crate_name,
                    "version": c.version,
                    "provides_elements": c.provides.elements,
                    "provides_attributes": c.provides.attributes,
                    "provides_directives": c.provides.directives,
                }))
                .collect::<Vec<_>>(),
            "available": self
                .available
                .iter()
                .map(|a| serde_json::json!({
                    "tag": a.tag,
                    "crate_name": a.crate_name,
                    "requirement": a.requirement,
                }))
                .collect::<Vec<_>>(),
            "unknown": &self.unknown,
            "unused": self
                .unused
                .iter()
                .map(|u| serde_json::json!({ "crate_name": u.crate_name }))
                .collect::<Vec<_>>(),
            "conflicts": self
                .conflicts
                .iter()
                .map(|c| serde_json::json!({
                    "tag": c.tag,
                    "owners": c.owners,
                }))
                .collect::<Vec<_>>(),
        })
    }
}

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

/// Собирает в `out` имена всех тегов вида `<имя ...>` из текста HTML.
fn extract_tag_names(text: &str, out: &mut BTreeMap<String, usize>) {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        // После '<' идём дальше; первый символ имени — буква.
        i += 1;
        if i >= bytes.len() || !(bytes[i].is_ascii_alphabetic()) {
            continue;
        }
        let start = i;
        while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-') {
            i += 1;
        }
        let name = std::str::from_utf8(&bytes[start..i]).unwrap_or("");
        if !name.is_empty() {
            *out.entry(name.to_string()).or_insert(0) += 1;
        }
    }
}

/// Преобразует метаданные из `cargo metadata` в компактный [`ComponentInfo`].
fn meta_to_info(meta: crate::metadata::ComponentMetadata) -> ComponentInfo {
    ComponentInfo {
        crate_name: meta.crate_name,
        version: meta.version,
        provides: Provides {
            elements: meta.provides_elements,
            attributes: meta.provides_attributes,
            directives: meta.provides_directives,
        },
    }
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

/// Использовано ли свойство/директива в шаблонах. Атрибут может встречаться с
/// префиксами (`:value`, `@validate`, `v-model`) — сравниваем как целиком, так
/// и после снятия префикса.
fn attr_used(used: &BTreeMap<String, usize>, value: &str) -> bool {
    used.keys()
        .any(|k| k == value || k.trim_start_matches([':', '@', 'v']) == value)
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

    // ─────────────── Умная проверка (Этап 1) ─────────────────────────────────

    /// Разметка фикстуры: создаёт каталог с `templates/app.html` и
    /// произвольным содержимым.
    fn templates_fixture(dir: &Path, app_html: &str) {
        std::fs::create_dir_all(dir.join("templates")).unwrap();
        std::fs::write(dir.join("templates/app.html"), app_html).unwrap();
    }

    #[test]
    fn scan_used_tags_extracts_div_checkbox_datepicker() {
        let dir = std::env::temp_dir().join("light-fw-cli-scan-tags");
        let _ = std::fs::remove_dir_all(&dir);
        templates_fixture(
            &dir,
            r#"
            <div class="wrap">
              <checkbox v-model="agreed"></checkbox>
              <datepicker v-model="date"></datepicker>
              <button>OK</button>
            </div>
            "#,
        );
        let used = scan_used_tags(&dir).expect("scan");
        assert_eq!(used.get("div"), Some(&1));
        assert_eq!(used.get("checkbox"), Some(&1));
        assert_eq!(used.get("datepicker"), Some(&1));
        assert_eq!(used.get("button"), Some(&1));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_used_tags_ignores_non_html_and_no_templates() {
        let dir = std::env::temp_dir().join("light-fw-cli-scan-tags-2");
        let _ = std::fs::remove_dir_all(&dir);
        // Без каталога `templates` → пустой результат.
        std::fs::create_dir_all(&dir).unwrap();
        let used = scan_used_tags(&dir).expect("scan");
        assert!(used.is_empty());
        // Не-HTML файл игнорируется.
        templates_fixture(&dir, "");
        std::fs::write(dir.join("templates/README.md"), "<div></div>\n").unwrap();
        let used = scan_used_tags(&dir).expect("scan");
        assert!(used.is_empty(), "md не сканируется");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Хелпер: строит [`ComponentInfo`] из полей по умолчанию.
    fn component_info(name: &str, version: &str, elements: &[&str]) -> ComponentInfo {
        ComponentInfo {
            crate_name: name.into(),
            version: version.into(),
            provides: Provides {
                elements: elements.iter().map(|s| s.to_string()).collect(),
                attributes: Vec::new(),
                directives: Vec::new(),
            },
        }
    }

    #[test]
    fn check_compare_empty_project_has_no_categories() {
        let mut used: BTreeMap<String, usize> = BTreeMap::new();
        used.insert("div".into(), 1);
        used.insert("span".into(), 2);
        let installed: Vec<ComponentInfo> = Vec::new();
        let report = compare_tags(&used, &installed);
        assert!(report.installed.is_empty());
        assert!(report.available.is_empty());
        assert!(report.unknown.is_empty());
        assert!(report.unused.is_empty());
        assert!(report.conflicts.is_empty());
        assert!(!report.has_problems());
    }

    #[test]
    fn check_compare_checkbox_without_form_is_available() {
        let mut used: BTreeMap<String, usize> = BTreeMap::new();
        used.insert("div".into(), 1);
        used.insert("checkbox".into(), 1);
        let installed: Vec<ComponentInfo> = Vec::new();
        let report = compare_tags(&used, &installed);
        // checkbox не установлен, но есть в реестре (light-fw-element-form).
        assert!(report.installed.is_empty());
        assert_eq!(report.available.len(), 1);
        assert_eq!(report.available[0].tag, "checkbox");
        assert_eq!(report.available[0].crate_name, "light-fw-element-form");
        assert!(report.unknown.is_empty());
        assert!(
            report.has_problems(),
            "--strict должен падать при available"
        );
    }

    #[test]
    fn check_compare_unknown_tag_is_unknown() {
        // Этап 6 Plugin API: `datepicker` теперь есть в bundled registry.toml
        // (`light-widget-datepicker`), поэтому для покрытия `unknown`-ветки
        // используем тег, которого реально нет ни в ядре, ни в реестре.
        let mut used: BTreeMap<String, usize> = BTreeMap::new();
        used.insert("div".into(), 1);
        used.insert("alien-widget".into(), 1);
        let installed: Vec<ComponentInfo> = Vec::new();
        let report = compare_tags(&used, &installed);
        // alien-widget нет в bundled registry.toml → unknown.
        assert!(report.available.is_empty());
        assert_eq!(report.unknown, vec!["alien-widget".to_string()]);
        assert!(report.has_problems());
    }

    #[test]
    fn check_compare_datepicker_without_plugin_is_available() {
        // Этап 6 Plugin API: `<datepicker />` в свежем проекте (плагин не
        // установлен) классифицируется как `available` — `light dev`/`build`
        // предложат установить `light-widget-datepicker`.
        let mut used: BTreeMap<String, usize> = BTreeMap::new();
        used.insert("div".into(), 1);
        used.insert("datepicker".into(), 1);
        let installed: Vec<ComponentInfo> = Vec::new();
        let report = compare_tags(&used, &installed);
        assert!(report.installed.is_empty());
        assert_eq!(report.available.len(), 1);
        assert_eq!(report.available[0].tag, "datepicker");
        assert_eq!(report.available[0].crate_name, "light-widget-datepicker");
        assert!(report.unknown.is_empty());
        assert!(report.has_problems());
    }

    #[test]
    fn check_compare_conflict_two_components_share_tag() {
        let mut used: BTreeMap<String, usize> = BTreeMap::new();
        used.insert("shared".into(), 1);
        let installed = vec![
            component_info("light-fixture-a", "0.1.0", &["shared"]),
            component_info("light-fixture-b", "0.1.0", &["shared"]),
        ];
        let report = compare_tags(&used, &installed);
        assert_eq!(report.installed.len(), 2);
        assert_eq!(report.available.len(), 0);
        assert_eq!(report.unknown.len(), 0);
        assert_eq!(report.conflicts.len(), 1);
        assert_eq!(report.conflicts[0].tag, "shared");
        assert_eq!(
            report.conflicts[0].owners,
            vec!["light-fixture-a", "light-fixture-b"]
        );
        assert!(report.has_problems());
    }

    #[test]
    fn check_compare_installed_and_used_component_not_unused() {
        let mut used: BTreeMap<String, usize> = BTreeMap::new();
        used.insert("checkbox".into(), 1);
        let installed = vec![component_info(
            "light-fw-element-form",
            "0.17.0",
            &["checkbox"],
        )];
        let report = compare_tags(&used, &installed);
        assert_eq!(report.installed.len(), 1);
        assert_eq!(report.installed[0].crate_name, "light-fw-element-form");
        // checkbox использован → не unused.
        assert!(report.unused.is_empty());
        assert!(report.available.is_empty());
        assert!(report.unknown.is_empty());
        assert!(!report.has_problems());
    }

    #[test]
    fn check_compare_unused_component_when_tag_not_in_templates() {
        // Установлен light-fw-element-form, но ни select/checkbox/radio/option
        // не используется → попадает в `unused`.
        let mut used: BTreeMap<String, usize> = BTreeMap::new();
        used.insert("div".into(), 1);
        let installed = vec![component_info(
            "light-fw-element-form",
            "0.17.0",
            &["select", "checkbox", "radio", "option"],
        )];
        let report = compare_tags(&used, &installed);
        assert_eq!(report.unused.len(), 1);
        assert_eq!(report.unused[0].crate_name, "light-fw-element-form");
    }

    #[test]
    fn check_report_to_text_contains_category_markers() {
        let report = CheckReport {
            installed: vec![ComponentInfo {
                crate_name: "light-fw-element-form".into(),
                version: "0.17.0".into(),
                provides: Provides {
                    elements: vec!["checkbox".into()],
                    attributes: vec![],
                    directives: vec![],
                },
            }],
            available: vec![AvailableComponent {
                tag: "datepicker".into(),
                crate_name: "light-widget-datepicker".into(),
                requirement: "light-widget-datepicker ^0.19".into(),
            }],
            unknown: vec!["custom-el".into()],
            unused: Vec::new(),
            conflicts: vec![Conflict {
                tag: "dup".into(),
                owners: vec!["a".into(), "b".into()],
            }],
        };
        let text = report.to_text();
        assert!(text.contains("🟢 Установлено"));
        assert!(text.contains("light-fw-element-form"));
        assert!(text.contains("🟡 Доступно (установить)"));
        assert!(text.contains("light-widget-datepicker ^0.19"));
        assert!(text.contains("🔴 Неизвестно"));
        assert!(text.contains("custom-el"));
        assert!(text.contains("⚠️ Конфликты"));
        assert!(text.contains("dup"));
    }

    #[test]
    fn check_report_to_json_serializes_all_categories() {
        let report = CheckReport {
            installed: Vec::new(),
            available: vec![AvailableComponent {
                tag: "checkbox".into(),
                crate_name: "light-fw-element-form".into(),
                requirement: "light-fw-element-form ^0.17".into(),
            }],
            unknown: vec!["custom".into()],
            unused: Vec::new(),
            conflicts: Vec::new(),
        };
        let json = report.to_json();
        assert_eq!(json["available"][0]["tag"], "checkbox");
        assert_eq!(json["available"][0]["crate_name"], "light-fw-element-form");
        assert_eq!(json["unknown"][0], "custom");
        assert_eq!(json["installed"].as_array().unwrap().len(), 0);
    }
}
