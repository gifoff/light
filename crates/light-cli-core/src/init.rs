//! Команда `light init`: создание скелета нового проекта Light.
//!
//! Модуль генерирует минимальный проект: `Cargo.toml` с зависимостью `light`,
//! самодостаточный `src/main.rs` (окно с текстом «Hello, Light!»), `.gitignore`
//! и `light.lock`. Внешние эффекты (проверка `cargo`) идут через трейт
//! [`CommandRunner`], поэтому логика тестируется без запуска процессов.

use std::path::PathBuf;
use std::str::FromStr;

use crate::error::{Error, Result};
use crate::lockfile::{LOCKFILE_NAME, Lockfile};
use crate::process::CommandRunner;

/// Версия мета-крейта `light`, которую прописывает `light init`.
pub const LIGHT_DEPENDENCY_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Содержимое `.gitignore` нового проекта.
pub const GITIGNORE: &str = "target/\n";

/// Дополнительный компонент, подключаемый флагом `--with`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum InitComponent {
    /// `light-fw-element-form`: `<checkbox>`, `<radio>`, `<select>`/`<option>`.
    Form,
    /// `light-fw-behavior-validation`: `required`, `:validator`, `@validate`.
    Validation,
}

impl InitComponent {
    /// Все поддерживаемые компоненты в порядке вывода в `Cargo.toml`.
    pub const ALL: [Self; 2] = [Self::Form, Self::Validation];

    /// Разбирает имя компонента из `--with`.
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "form" => Ok(Self::Form),
            "validation" => Ok(Self::Validation),
            other => Err(Error::Init(format!(
                "неизвестный компонент `{other}` (поддерживаются: form, validation)"
            ))),
        }
    }

    /// Имя крейта-компонента.
    pub fn crate_name(self) -> &'static str {
        match self {
            Self::Form => "light-fw-element-form",
            Self::Validation => "light-fw-behavior-validation",
        }
    }

    /// Фича мета-крейта `light`, включающая компонент.
    pub fn light_feature(self) -> &'static str {
        match self {
            Self::Form => "elements-form",
            Self::Validation => "validation",
        }
    }
}

impl FromStr for InitComponent {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self> {
        Self::parse(value)
    }
}

/// Параметры команды `light init`.
#[derive(Debug, Clone)]
pub struct InitOptions {
    /// Имя проекта (оно же имя крейта).
    pub name: String,
    /// Каталог проекта; по умолчанию — `./<name>`.
    pub path: Option<PathBuf>,
    /// Только `Cargo.toml` и `src/main.rs` (без `.gitignore` и `light.lock`).
    pub minimal: bool,
    /// Дополнительные компоненты (`--with`).
    pub components: Vec<InitComponent>,
}

/// Что будет создано (результат [`plan`]).
#[derive(Debug, Clone)]
pub struct InitPlan {
    /// Каталог проекта.
    pub directory: PathBuf,
    /// Файлы проекта: относительный путь и содержимое.
    pub files: Vec<(PathBuf, String)>,
    /// Создавать ли `light.lock`.
    pub write_lockfile: bool,
}

impl InitPlan {
    /// Человекочитаемый план.
    pub fn to_text(&self) -> String {
        let mut out = format!("Проект в {}\n", self.directory.display());
        for (path, _) in &self.files {
            out.push_str(&format!("  + {}\n", path.display()));
        }
        if self.write_lockfile {
            out.push_str(&format!("  + {}\n", LOCKFILE_NAME));
        }
        out
    }
}

/// Итог создания проекта.
#[derive(Debug, Clone)]
pub struct InitReport {
    /// Каталог проекта.
    pub directory: PathBuf,
    /// Созданные файлы (относительные пути).
    pub files: Vec<PathBuf>,
    /// Создан ли `light.lock`.
    pub lockfile: bool,
}

impl InitReport {
    /// Человекочитаемый отчёт с подсказкой о запуске.
    pub fn to_text(&self) -> String {
        let mut out = format!("✓ Проект создан в {}\n", self.directory.display());
        for path in &self.files {
            out.push_str(&format!("  {}\n", path.display()));
        }
        if self.lockfile {
            out.push_str(&format!("  {}\n", LOCKFILE_NAME));
        }
        out.push_str(&format!(
            "\nДалее: cd \"{}\" && cargo run\n",
            self.directory.display()
        ));
        out
    }
}

/// Проверяет, что `name` — допустимое имя крейта.
///
/// Разрешены латинские буквы, цифры, `-` и `_`; первый символ — буква или `_`.
pub fn validate_name(name: &str) -> Result<()> {
    if name.is_empty() || name.len() > 64 {
        return Err(Error::Init(
            "имя проекта должно быть длиной от 1 до 64 символов".to_string(),
        ));
    }
    let mut chars = name.chars();
    let first = chars.next().expect("непустое имя");
    if !(first.is_ascii_alphabetic() || first == '_') {
        return Err(Error::Init(format!(
            "имя `{name}` должно начинаться с латинской буквы или `_`"
        )));
    }
    if let Some(bad) = name
        .chars()
        .find(|c| !(c.is_ascii_alphanumeric() || *c == '-' || *c == '_'))
    {
        return Err(Error::Init(format!(
            "недопустимый символ `{bad}` в имени `{name}` (разрешены латинские буквы, цифры, `-`, `_`)"
        )));
    }
    Ok(())
}

/// Целевой каталог проекта из параметров.
pub fn project_dir(options: &InitOptions) -> PathBuf {
    match &options.path {
        Some(path) => path.clone(),
        None => PathBuf::from(&options.name),
    }
}

/// Компоненты без дублей в порядке [`InitComponent::ALL`].
fn normalized_components(components: &[InitComponent]) -> Vec<InitComponent> {
    InitComponent::ALL
        .into_iter()
        .filter(|component| components.contains(component))
        .collect()
}

/// Содержимое `Cargo.toml` нового проекта.
pub fn cargo_toml(name: &str, components: &[InitComponent]) -> String {
    let components = normalized_components(components);
    let mut text = format!(
        "[package]\n\
         name = \"{name}\"\n\
         version = \"0.1.0\"\n\
         edition = \"2021\"\n\
         \n\
         [dependencies]\n"
    );

    if components.is_empty() {
        text.push_str(&format!("light = \"{LIGHT_DEPENDENCY_VERSION}\"\n"));
        return text;
    }

    let features = components
        .iter()
        .map(|component| format!("\"{}\"", component.light_feature()))
        .collect::<Vec<_>>()
        .join(", ");
    text.push_str(&format!(
        "light = {{ version = \"{LIGHT_DEPENDENCY_VERSION}\", features = [{features}] }}\n"
    ));
    for component in &components {
        text.push_str(&format!(
            "{} = \"{LIGHT_DEPENDENCY_VERSION}\"\n",
            component.crate_name()
        ));
    }
    text
}

/// Закомментированный пример формы для проектов с компонентом `form`.
const FORM_EXAMPLE: &str = r#"
// Пример формы (компонент `form`):
//
// #[template("templates/form.html")]
// #[proxy]
// struct SignUp {
//     email: String,
//     agree: bool,
// }
//
// impl SignUp {
//     fn submit(&self) {
//         if self.agree() {
//             println!("email: {}", self.email());
//         }
//     }
// }
//
// templates/form.html:
// <window title="Sign up" width="360" height="240">
//     <div direction="vertical" padding="12">
//         <text v-model="email" placeholder="Email" required />
//         <checkbox v-model="agree">Согласен с условиями</checkbox>
//         <button @click="submit" :disabled="!agree">Отправить</button>
//     </div>
// </window>
"#;

/// Содержимое `src/main.rs` — минимальное приложение из одного окна.
pub fn main_rs(components: &[InitComponent]) -> String {
    let mut text = r#"use light::prelude::*;

struct App;

impl Component for App {
    fn render(&self) -> VElement {
        VElement::container(
            "",
            NodeKind::Window,
            vec![VElement::container(
                "0",
                NodeKind::Column,
                vec![VElement::leaf(
                    "0.0",
                    NodeKind::Text {
                        content: "Hello, Light!".to_string(),
                    },
                )],
            )],
        )
        .with_window(WindowConfig {
            title: "Hello, Light!".to_string(),
            width: 320,
            height: 200,
            ..Default::default()
        })
    }
}

fn main() -> light::Result<()> {
    light::run(App)
}
"#
    .to_string();

    if components.contains(&InitComponent::Form) {
        text.push_str(FORM_EXAMPLE);
    }
    text
}

/// Строит план создания проекта, проверяя, что каталог свободен.
///
/// Ошибка, если каталог существует и не пуст.
pub fn plan(options: &InitOptions) -> Result<InitPlan> {
    validate_name(&options.name)?;
    let directory = project_dir(options);

    if directory.is_dir() {
        let mut entries = std::fs::read_dir(&directory)?;
        if entries.next().is_some() {
            return Err(Error::Init(format!(
                "каталог `{}` не пуст — укажите другое имя или `--path`",
                directory.display()
            )));
        }
    } else if directory.exists() {
        return Err(Error::Init(format!(
            "`{}` существует и не является каталогом",
            directory.display()
        )));
    }

    let components = normalized_components(&options.components);
    let mut files = vec![
        (
            PathBuf::from("Cargo.toml"),
            cargo_toml(&options.name, &components),
        ),
        (PathBuf::from("src").join("main.rs"), main_rs(&components)),
    ];
    if !options.minimal {
        files.push((PathBuf::from(".gitignore"), GITIGNORE.to_string()));
    }

    Ok(InitPlan {
        directory,
        files,
        write_lockfile: !options.minimal,
    })
}

/// Проверяет, что `cargo` доступен в `PATH`.
pub fn require_cargo(runner: &dyn CommandRunner) -> Result<()> {
    match runner.run("cargo", &["--version".to_string()], None) {
        Ok(_) => Ok(()),
        Err(Error::CommandNotFound(_)) => Err(Error::Init(
            "`cargo` не найден в PATH — установите Rust (https://rustup.rs)".to_string(),
        )),
        Err(error) => Err(error),
    }
}

/// Записывает файлы плана на диск, создавая каталоги по мере необходимости.
pub fn scaffold(plan: &InitPlan) -> Result<()> {
    for (relative, content) in &plan.files {
        let path = plan.directory.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, content)?;
    }
    Ok(())
}

/// Полный цикл: проверка `cargo` → план → запись файлов → `light.lock`.
pub fn apply(runner: &dyn CommandRunner, options: &InitOptions) -> Result<InitReport> {
    require_cargo(runner)?;
    let plan = plan(options)?;
    scaffold(&plan)?;

    let lockfile = if plan.write_lockfile {
        let lock = Lockfile::from_project(&plan.directory)?;
        lock.write(&plan.directory)?;
        true
    } else {
        false
    };

    Ok(InitReport {
        directory: plan.directory,
        files: plan.files.into_iter().map(|(path, _)| path).collect(),
        lockfile,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::CommandOutput;
    use std::path::Path;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn scratch(name: &str) -> PathBuf {
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "light-fw-cli-init-{name}-{}-{unique}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    struct StubRunner {
        found: bool,
    }

    impl CommandRunner for StubRunner {
        fn run(
            &self,
            program: &str,
            _args: &[String],
            _cwd: Option<&Path>,
        ) -> Result<CommandOutput> {
            if self.found {
                Ok(CommandOutput {
                    status: 0,
                    stdout: "cargo 1.99.0".to_string(),
                    stderr: String::new(),
                })
            } else {
                Err(Error::CommandNotFound(program.to_string()))
            }
        }
    }

    #[test]
    fn rejects_invalid_names() {
        assert!(validate_name("my-app").is_ok());
        assert!(validate_name("app_2").is_ok());
        assert!(validate_name("").is_err());
        assert!(validate_name("2app").is_err());
        assert!(validate_name("my app").is_err());
        assert!(validate_name("app!").is_err());
    }

    #[test]
    fn plan_default_creates_all_files() {
        let root = scratch("plan-default");
        let dir = root.join("cool-app");
        let options = InitOptions {
            name: "cool-app".to_string(),
            path: Some(dir.clone()),
            minimal: false,
            components: Vec::new(),
        };
        let plan = plan(&options).expect("plan");
        let names: Vec<String> = plan
            .files
            .iter()
            .map(|(path, _)| path.to_string_lossy().replace('\\', "/"))
            .collect();
        assert!(names.contains(&"Cargo.toml".to_string()));
        assert!(names.contains(&"src/main.rs".to_string()));
        assert!(names.contains(&".gitignore".to_string()));
        assert!(plan.write_lockfile);
    }

    #[test]
    fn plan_minimal_omits_gitignore_and_lockfile() {
        let root = scratch("plan-minimal");
        let options = InitOptions {
            name: "tiny".to_string(),
            path: Some(root.join("tiny")),
            minimal: true,
            components: Vec::new(),
        };
        let plan = plan(&options).expect("plan");
        let names: Vec<String> = plan
            .files
            .iter()
            .map(|(path, _)| path.to_string_lossy().replace('\\', "/"))
            .collect();
        assert!(!names.contains(&".gitignore".to_string()));
        assert!(!plan.write_lockfile);
    }

    #[test]
    fn plan_rejects_non_empty_directory() {
        let root = scratch("plan-nonempty");
        let target = root.join("busy");
        std::fs::create_dir_all(&target).expect("dir");
        std::fs::write(target.join("file.txt"), "x").expect("file");
        let options = InitOptions {
            name: "busy".to_string(),
            path: Some(target),
            minimal: false,
            components: Vec::new(),
        };
        let error = plan(&options).expect_err("должна быть ошибка");
        assert!(error.to_string().contains("не пуст"), "{error}");
    }

    #[test]
    fn cargo_toml_pins_light_version() {
        let text = cargo_toml("demo", &[]);
        assert!(text.contains("name = \"demo\""));
        assert!(text.contains(&format!("light = \"{LIGHT_DEPENDENCY_VERSION}\"")));
        assert!(!text.contains("features ="));
    }

    #[test]
    fn cargo_toml_adds_form_dependency_and_feature() {
        let text = cargo_toml("demo", &[InitComponent::Form]);
        assert!(
            text.contains(&format!(
                "light = {{ version = \"{LIGHT_DEPENDENCY_VERSION}\", features = [\"elements-form\"] }}"
            )),
            "{text}"
        );
        assert!(
            text.contains(&format!(
                "light-fw-element-form = \"{LIGHT_DEPENDENCY_VERSION}\""
            )),
            "{text}"
        );
        assert!(!text.contains("light-fw-behavior-validation"), "{text}");
    }

    #[test]
    fn cargo_toml_adds_both_dependencies_and_deduplicates() {
        let text = cargo_toml(
            "demo",
            &[
                InitComponent::Validation,
                InitComponent::Form,
                InitComponent::Form,
            ],
        );
        assert!(
            text.contains(&format!(
                "light = {{ version = \"{LIGHT_DEPENDENCY_VERSION}\", features = [\"elements-form\", \"validation\"] }}"
            )),
            "{text}"
        );
        assert_eq!(text.matches("light-fw-element-form =").count(), 1, "{text}");
        assert_eq!(
            text.matches("light-fw-behavior-validation =").count(),
            1,
            "{text}"
        );
    }

    #[test]
    fn main_rs_is_self_contained() {
        let text = main_rs(&[]);
        assert!(text.contains("Hello, Light!"));
        assert!(!text.contains("include_str!"));
        assert!(!text.contains("#[template"));
        assert!(!text.contains("Пример формы"));
    }

    #[test]
    fn main_rs_includes_form_example_only_for_form_component() {
        let text = main_rs(&[InitComponent::Form]);
        assert!(text.contains("Пример формы"), "{text}");
        let validation_only = main_rs(&[InitComponent::Validation]);
        assert!(
            !validation_only.contains("Пример формы"),
            "{validation_only}"
        );
    }

    #[test]
    fn require_cargo_reports_missing_tool() {
        let error = require_cargo(&StubRunner { found: false }).expect_err("нет cargo");
        assert!(error.to_string().contains("cargo"), "{error}");
        assert!(require_cargo(&StubRunner { found: true }).is_ok());
    }

    #[test]
    fn apply_creates_project_and_lockfile() {
        let root = scratch("apply");
        let dir = root.join("my-app");
        let options = InitOptions {
            name: "my-app".to_string(),
            path: Some(dir.clone()),
            minimal: false,
            components: Vec::new(),
        };
        let report = apply(&StubRunner { found: true }, &options).expect("apply");
        assert!(report.lockfile);
        assert!(dir.join("Cargo.toml").is_file());
        assert!(dir.join("src").join("main.rs").is_file());
        assert!(dir.join(".gitignore").is_file());
        assert!(dir.join(LOCKFILE_NAME).is_file());

        let lock = Lockfile::read(&dir).expect("read lock");
        assert_eq!(lock.core, LIGHT_DEPENDENCY_VERSION);
    }

    #[test]
    fn apply_minimal_skips_lockfile() {
        let root = scratch("apply-minimal");
        let dir = root.join("tiny");
        let options = InitOptions {
            name: "tiny".to_string(),
            path: Some(dir.clone()),
            minimal: true,
            components: Vec::new(),
        };
        let report = apply(&StubRunner { found: true }, &options).expect("apply");
        assert!(!report.lockfile);
        assert!(dir.join("Cargo.toml").is_file());
        assert!(!dir.join(".gitignore").exists());
        assert!(!dir.join(LOCKFILE_NAME).exists());
    }

    #[test]
    fn apply_fails_without_cargo_before_writing() {
        let root = scratch("apply-nocargo");
        let dir = root.join("nope");
        let options = InitOptions {
            name: "nope".to_string(),
            path: Some(dir.clone()),
            minimal: false,
            components: Vec::new(),
        };
        assert!(apply(&StubRunner { found: false }, &options).is_err());
        assert!(!dir.exists(), "каталог не должен создаваться");
    }
}
