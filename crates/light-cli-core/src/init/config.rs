/// Версия мета-крейта `light-fw`, которую прописывает `light init`.
pub const LIGHT_DEPENDENCY_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Minor-диапазон версии (`major.minor`), используемый в `Cargo.toml` нового
/// проекта: пользователь всегда получает latest `major.minor.x`, не привязываясь
/// к конкретному yanked-патчу.
pub fn minor_version() -> String {
    LIGHT_DEPENDENCY_VERSION
        .rsplit_once('.')
        .map(|(prefix, _)| prefix.to_string())
        .unwrap_or_else(|| LIGHT_DEPENDENCY_VERSION.to_string())
}

/// Содержимое `.gitignore` нового проекта.
pub const GITIGNORE: &str = "target/\n";

/// Алиасы `cargo dev` / `cargo prod`: отладочный режим (DevTools) и релиз.
pub const CARGO_CONFIG: &str =
    "[alias]\ndev = \"run --features light-fw/dev-mode\"\nprod = \"run --release\"\n";

/// Конфигурация `light.config.json`: профили `dev` (DevTools на 9229, debug),
/// `prod` (DevTools выключен, error) и `registry` (базовые URL реестра для
/// `light search`; по умолчанию crates.io, см. `docs_url`/`download_url`).
pub const LIGHT_CONFIG: &str = r#"{
    "dev": {
        "devtools": {
            "enabled": true,
            "port": 9229
        },
        "log_level": "debug"
    },
    "prod": {
        "devtools": {
            "enabled": false
        },
        "log_level": "error"
    },
    "registry": {
        "url": "https://crates.io",
        "download_url": "https://static.crates.io",
        "docs_url": "https://docs.rs"
    }
}
"#;

pub(crate) const COUNTER_MAIN: &str = include_str!("../../assets/counter/src/main.rs");
pub(crate) const COUNTER_HTML: &str = include_str!("../../assets/counter/templates/counter.html");
pub(crate) const TODO_MAIN: &str = include_str!("../../assets/todo/src/main.rs");
pub(crate) const TODO_HTML: &str = include_str!("../../assets/todo/templates/todo.html");
pub(crate) const CALCULATOR_MAIN: &str = include_str!("../../assets/calculator/src/main.rs");
pub(crate) const CALCULATOR_HTML: &str =
    include_str!("../../assets/calculator/templates/calculator.html");
