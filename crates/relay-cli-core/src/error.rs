use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("TOML parse error: {0}")]
    Toml(#[from] toml_edit::TomlError),

    #[error("TOML serialize error: {0}")]
    TomlSer(#[from] toml_edit::ser::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Semver error: {0}")]
    Semver(#[from] semver::Error),

    #[error("Walkdir error: {0}")]
    Walkdir(#[from] walkdir::Error),

    #[error("Ignore error: {0}")]
    Ignore(#[from] ignore::Error),

    #[error("Template parse error: {0}")]
    TemplateParse(String),

    #[error("Manifest error: {0}")]
    Manifest(String),

    /// Внешний процесс завершился с ненулевым кодом.
    #[error("команда `{program}` завершилась с кодом {code}: {stderr}")]
    CommandFailed {
        program: String,
        code: i32,
        stderr: String,
    },

    /// Исполняемый файл не найден в `PATH`.
    #[error("команда `{0}` не найдена (установлен ли инструмент?)")]
    CommandNotFound(String),

    /// Компонент не найден ни в workspace, ни в vendor, ни в реестре.
    #[error("компонент `{0}` не найден")]
    ComponentNotFound(String),

    /// Компонент несовместим с текущим ядром проекта или другими компонентами.
    #[error("компонент `{component}` несовместим: {reason}")]
    Incompatible { component: String, reason: String },

    /// Компонент не подключён к проекту.
    #[error("компонент `{0}` не установлен")]
    NotInstalled(String),

    /// Конфликт между компонентами (`conflicts`).
    #[error("конфликт компонентов: {0}")]
    Conflict(String),

    /// От него зависят другие компоненты.
    #[error("от компонента `{0}` зависят другие компоненты — удалите их вместе")]
    InUse(String),

    /// Операция отменена пользователем.
    #[error("операция отменена")]
    Aborted,

    /// Требуется сеть, но включён `--offline`.
    #[error(
        "компонент `{0}` не найден в vendor/, установите его без `--offline` на машине с сетью"
    )]
    Offline(String),

    /// Ошибка работы с `relay.lock`.
    #[error("ошибка relay.lock: {0}")]
    Lockfile(String),

    /// Ошибка резервного копирования/восстановления.
    #[error("ошибка резервной копии: {0}")]
    Backup(String),

    /// Ошибка публикации: граф зависимостей, версии, pre-flight.
    #[error("ошибка публикации: {0}")]
    Publish(String),

    /// Версия крейта уже опубликована на crates.io.
    #[error(
        "crate {name} v{version} already published; bump version in Cargo.toml before publishing"
    )]
    AlreadyPublished { name: String, version: String },

    /// Сетевой запрос к crates.io завершился ошибкой.
    #[cfg(feature = "network")]
    #[error("сетевой запрос не удался: {0}")]
    Http(#[from] reqwest::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
