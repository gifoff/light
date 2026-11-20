use std::path::PathBuf;

use super::config::{CARGO_CONFIG, GITIGNORE, LIGHT_CONFIG};
use super::generate::{
    APP_HTML, cargo_toml, main_rs, normalized_components, project_dir, validate_name,
};
use super::types::{InitOptions, InitPlan, InitReport};
use crate::error::{Error, Result};
use crate::lockfile::Lockfile;
use crate::process::CommandRunner;

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
    let main = match options.template {
        Some(template) => template.main_rs().to_string(),
        None => main_rs(&components),
    };
    let mut files = vec![
        (
            PathBuf::from("Cargo.toml"),
            cargo_toml(&options.name, &components),
        ),
        (PathBuf::from("src").join("main.rs"), main),
        // Алиасы `cargo dev` / `cargo prod` и конфигурация DevTools/логирования.
        (
            PathBuf::from(".cargo").join("config.toml"),
            CARGO_CONFIG.to_string(),
        ),
        (PathBuf::from("light.config.json"), LIGHT_CONFIG.to_string()),
    ];
    if let Some(template) = options.template {
        for (path, content) in template.template_files() {
            files.push((PathBuf::from(path), (*content).to_string()));
        }
    } else {
        // Проект без `--template`: `main.rs` ссылается на `templates/app.html`.
        files.push((
            PathBuf::from("templates").join("app.html"),
            APP_HTML.to_string(),
        ));
    }
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
