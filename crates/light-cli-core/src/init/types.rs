use std::path::PathBuf;
use std::str::FromStr;

use super::config::*;
use crate::error::{Error, Result};
use crate::lockfile::LOCKFILE_NAME;

/// Готовый пример, который `light init --template` кладёт в новый проект.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitTemplate {
    /// Счётчик с реактивным состоянием.
    Counter,
    /// Список задач с `v-model`, `v-if` и `v-for`.
    Todo,
    /// Калькулятор с обработчиками кнопок.
    Calculator,
}

impl InitTemplate {
    /// Все поддерживаемые шаблоны.
    pub const ALL: [Self; 3] = [Self::Counter, Self::Todo, Self::Calculator];

    /// Разбирает имя шаблона из `--template`.
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "counter" => Ok(Self::Counter),
            "todo" => Ok(Self::Todo),
            "calculator" => Ok(Self::Calculator),
            other => Err(Error::Init(format!(
                "неизвестный шаблон `{other}` (поддерживаются: counter, todo, calculator)"
            ))),
        }
    }

    /// Содержимое `src/main.rs` шаблона.
    pub fn main_rs(self) -> &'static str {
        match self {
            Self::Counter => COUNTER_MAIN,
            Self::Todo => TODO_MAIN,
            Self::Calculator => CALCULATOR_MAIN,
        }
    }

    /// Дополнительные HTML-шаблоны: путь в проекте и содержимое.
    pub fn template_files(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Self::Counter => &[("templates/counter.html", COUNTER_HTML)],
            Self::Todo => &[("templates/todo.html", TODO_HTML)],
            Self::Calculator => &[("templates/calculator.html", CALCULATOR_HTML)],
        }
    }
}

impl FromStr for InitTemplate {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self> {
        Self::parse(value)
    }
}

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

    /// Фича мета-крейта `light-fw`, включающая компонент.
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
    /// Готовый шаблон приложения (`--template`).
    pub template: Option<InitTemplate>,
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
