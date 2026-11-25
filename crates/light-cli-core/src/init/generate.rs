use std::path::PathBuf;

use super::config::minor_version;
use super::types::{InitComponent, InitOptions};
use crate::error::{Error, Result};

/// Проверяет, что `name` — допустимое имя крейта.
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
pub(crate) fn normalized_components(components: &[InitComponent]) -> Vec<InitComponent> {
    InitComponent::ALL
        .into_iter()
        .filter(|component| components.contains(component))
        .collect()
}

/// Содержимое `Cargo.toml` нового проекта.
/// Шаблон-заглушка для проекта без `--template`: минимальное окно с
/// приветствием, центрированным в контентной области.
pub(crate) const APP_HTML: &str = r#"<window title="Hello, Light!" width="480" height="320">
    <div width="100%" height="100%" direction="vertical" justify-content="center" align-items="center" padding="16">
        <h1>Hello, Light!</h1>
    </div>
</window>
"#;

/// Содержимое `Cargo.toml` для нового проекта: зависимости `light-fw`,
/// компоненты и фича `dev-mode` (алиас `light-fw/dev-mode`).
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
        text.push_str(&format!("light-fw = \"{}\"\n", minor_version()));
    } else {
        let features = components
            .iter()
            .map(|component| format!("\"{}\"", component.light_feature()))
            .collect::<Vec<_>>()
            .join(", ");
        let v = minor_version();
        text.push_str(&format!(
            "light-fw = {{ version = \"{v}\", features = [{features}] }}\n"
        ));
        for component in &components {
            text.push_str(&format!("{} = \"{v}\"\n", component.crate_name()));
        }
    }

    // Фича `dev-mode` — алиас для `light-fw/dev-mode` (runtime-templates, hot
    // reload). Необходима для `light dev` и `.cargo/config.toml`.
    text.push_str("\n[features]\ndev-mode = [\"light-fw/dev-mode\"]\n");
    // ТЗ 0.16.1, задача 2: пустой `[workspace]` в самом конце — чтобы проект
    // не «попадал» в чужой workspace, если создан внутри папки с Cargo.toml
    // («current package believes it's in a workspace when it's not»).
    text.push_str("\n[workspace]\n");
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

/// Содержимое `src/main.rs` — минимальное приложение с HTML-шаблоном
/// (`templates/app.html`) и пустой прокси-структурой.
pub fn main_rs(components: &[InitComponent]) -> String {
    let mut text = r#"#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use light_fw::prelude::*;

#[template("templates/app.html")]
#[proxy]
struct App {}

fn main() -> light_fw::Result<()> {
    light_fw::run_with_config("light.config.json", App::new())
}
"#
    .to_string();

    if components.contains(&InitComponent::Form) {
        text.push_str(FORM_EXAMPLE);
    }
    text
}

/// ТЗ 0.21.0: `Cargo.toml` headless-проекта. Зависимость только
/// `light-fw-core` с `default-features = false` (нет winit/window), поэтому
/// проект компилируется без GUI и подходит для AI-агентов и serverless-скриптов.
/// `light-fw` (мета-крейт) НЕ используется — это прямой доступ к реактивности.
///
/// Версия `light-fw-core` зафиксирована как `"0.21"` по ТЗ (буквально); это
/// не `minor_version()`, потому что:
/// 1. Headless-режим (без winit) появляется только в 0.21.x — в 0.20.2
///    `light-gui` является необязательной зависимостью, и `cargo tree | grep
///    winit` найдёт результат даже при `default-features = false`.
/// 2. `minor_version()` сегодня возвращает `"0.20"` (workspace version
///    `0.20.2`) — если оставить его, порождаемый проект будет сломан
///    (зависит от ещё не опубликованной фичи на версии, где она нет).
///
/// Буквальный текст ТЗ: «`light-fw-core = { version = "0.21",
/// default-features = false }`». Если на момент публикации 0.21.0 нужно
/// переключить на `minor_version()` — отдельный коммит `chore(cli):
/// headless → dynamic version`.
pub fn cargo_toml_headless(name: &str) -> String {
    const HEADLESS_FW_CORE: &str = "0.21";
    format!(
        "[package]\n\
         name = \"{name}\"\n\
         version = \"0.1.0\"\n\
         edition = \"2021\"\n\
         \n\
         [dependencies]\n\
         light-fw-core = {{ version = \"{HEADLESS_FW_CORE}\", default-features = false }}\n\
         \n\
         [workspace]\n"
    )
}

/// ТЗ 0.21.0: `main.rs` headless-проекта. Минимальный пример с `Property`
/// (реактивная ячейка значения) и `watch` (подписка на изменения в стиле
/// `watchEffect` из Vue 3) — без `#[template]`, без `run()`, без GUI.
/// `watch` принимает замыкание без аргументов: каждое чтение `.get()` внутри
/// него регистрируется как зависимость, а при любом изменении читаемых свойств
/// замыкание выполняется заново.
pub const HEADLESS_MAIN: &str = r#"//! Headless Light: реактивность без GUI.
//!
//! Пример `Property` + `watch` в `light-fw-core` (default-features = false).
//! `Property<T>` — реактивная ячейка (аналог `ref` в Vue), `watch(f)` —
//! подписка, которая исполняет `f` один раз, запоминает прочитанные свойства
//! и повторяет `f` при их изменении (аналог `watchEffect`).
//!
//! Здесь обработчик печатает текущее значение счётчика; `set`/`update`
//! уведомляют все зависимые эффекты и запрашивают перерисовку (в headless
//! режиме — просто вызов, без окна).

use light_fw_core::reactive::{watch, Property};

fn main() {
    // Реактивный счётчик.
    let count = Property::new(0i64);

    // Подписка: печатаем новое значение при каждом изменении.
    // Клон `Property` разделяет ту же ячейку (Rc под капотом) — поэтому
    // `move`-замыкание может захватить клон и жить в `Watcher`.
    let _w = watch({
        let count = count.clone();
        move || {
            // любое чтение здесь — зависимость эффекта
            let value = count.get();
            println!("count = {value}");
        }
    });

    // Имитируем изменение (в реальном приложении это будет UI/команда/IO).
    count.set(count.get() + 1);
    count.update(|v| *v += 1);

    // Явно останавливаем подписку (в конце main Watcher::drop тоже вызовет stop).
    println!("done, final count = {}", count.peek());
    std::mem::drop(_w);
}
"#;
