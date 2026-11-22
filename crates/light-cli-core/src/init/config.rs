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

/// Шпаргалка для AI-агентов, создаваемых в корне каждого нового проекта
/// (`light init`). Напоминает агентам не читать исходники фреймворка, а опираться
/// на документацию и команды `light devtools`, и фиксирует частые грабли
/// (`<text>` — поле ввода, `<image>` требует `image-support`).
pub const AGENT_MD: &str = r#"# Инструкция для AI-агентов (Light Framework)

Привет, AI! Если ты читаешь это, значит, ты работаешь над проектом
на Light. **НЕ ЧИТАЙ ИСХОДНИКИ ФРЕЙМВОРКА** (light-vdom, light-macros
и т.д.). Вся необходимая информация есть здесь.

## 1. Синтаксис шаблонов (Важно!)
- `<text>` — это **поле ввода** (требует `v-model`). Он не может
  содержать дочерних элементов. Для простого текста используй
  `<div>` или `<span>`: `<div font-size="24">{{ count }}</div>`.
- `<image src="path" width="..." height="..." />` — требует фичи
  `image-support` в Cargo.toml.
- Условный рендеринг: `<div v-if="condition">...</div>`.
  Условие — это имя метода или поля (например, `is_home`).

## 2. Как запустить и протестировать (DevTools)
**НИКОГДА** не используй `cargo run` и не блокируй терминал!

1. Запусти в фоне (PowerShell):
   `Start-Process powershell -ArgumentList "-NoExit", "-Command", "cd .; light dev --agent"`
2. Подожди 5-10 секунд (пока окно откроется).
3. Используй команды `light devtools`:
   - `light devtools shot --path shots` (сделать скриншот)
   - `light devtools find --tag button --text "Click"` (найти элемент)
   - `light devtools click --key <key>` (кликнуть)
   - `light devtools state` (посмотреть JSON-состояние)
   - `light devtools type --key <key> "text"` (ввести текст)
4. Останови сессию: `light devtools session stop`.

## 3. Документация
Полное руководство: https://github.com/gifoff/light/blob/master/docs/guide.md
"#;

pub(crate) const COUNTER_MAIN: &str = include_str!("../../assets/counter/src/main.rs");
pub(crate) const COUNTER_HTML: &str = include_str!("../../assets/counter/templates/counter.html");
pub(crate) const TODO_MAIN: &str = include_str!("../../assets/todo/src/main.rs");
pub(crate) const TODO_HTML: &str = include_str!("../../assets/todo/templates/todo.html");
pub(crate) const CALCULATOR_MAIN: &str = include_str!("../../assets/calculator/src/main.rs");
pub(crate) const CALCULATOR_HTML: &str =
    include_str!("../../assets/calculator/templates/calculator.html");
