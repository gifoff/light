//! # Light — реактивный графический интерфейс для Rust
//!
//! Light — библиотека со **своим окном** (winit + программная отрисовка)
//! и реактивностью в духе Vue 3:
//!
//! * атрибут `#[proxy]` превращает структуру в реактивный класс: свойства
//!   читаются и записываются через геттеры/сеттеры, которые уведомляют
//!   обозревателя;
//! * атрибут `#[template("...")]` компилирует HTML-шаблон с директивами
//!   (`{{ … }}`, `v-model`, `@click`, `v-if`, `:disabled`, `<include>`)
//!   на этапе сборки — шаблон **полностью замещает** форму компонента;
//! * отрисовка построена на виртуальном дереве: каждое изменение свойства
//!   приводит к пересборке дерева и сравнению с предыдущим
//!   ([`vdom::diff`]), в результате чего обновляются **только реально
//!   изменившиеся элементы** — неизменённые узлы не перерисовываются;
//! * шаблоны могут включать другие шаблоны через `<include src="...">` —
//!   так главная форма собирается из внутренних страниц и разделов.
//!
//! ## Компоненты
//!
//! Ядро библиотеки — крейт [`light-fw-core`](light_fw_core). Дополнительные
//! возможности подключаются как отдельные компоненты и фичи:
//!
//! * [`light-fw-element-form`](light_fw_element_form) (фича `elements-form`) —
//!   элементы `<checkbox>`, `<radio>`, `<select>`/`<option>`;
//! * [`light-fw-behavior-validation`](light_fw_behavior_validation)
//!   (фича `validation`) — `required`, `:validator` и `@validate`.
//!
//! ## Пример
//!
//! ```ignore
//! use light_fw::prelude::*;
//!
//! // Приложение из документации.
//! #[template("templates/doc_example.html")]
//! #[proxy]
//! struct App {
//!     title: String,
//!     count: i64,
//!     name: String,
//!     show_greeting: bool,
//! }
//!
//! impl App {
//!     fn increment(&self) {
//!         self.set_count(self.count() + 1);
//!     }
//!
//!     fn is_zero(&self) -> bool {
//!         self.count() == 0
//!     }
//!
//!     fn reset(&self) {
//!         self.set_count(0);
//!     }
//!
//!     fn toggle_greeting(&self) {
//!         self.set_show_greeting(!self.show_greeting());
//!     }
//! }
//!
//! fn main() -> light_fw::Result<()> {
//!     light_fw::run(App::new())
//! }
//! ```
//!
//! Шаблон `templates/doc_example.html`:
//!
//! ```html
//! <div>
//!     <text>{{ title }}</text>
//!     <button @click="increment" :disabled="is_zero">+1 ({{ count }})</button>
//!     <button @click="reset">Сброс</button>
//! </div>
//! ```

#![deny(missing_docs)]
#![forbid(unsafe_code)]

// Позволяет генерируемому коду обращаться к `::light_fw::…` изнутри самой
// библиотеки (см. код, генерируемый `light-fw-macros`).
extern crate self as light_fw;

#[cfg(feature = "devtools")]
use serde::Deserialize;

// Основной API доступен всегда, без cfg (0.15.1). Глоб-реэкспорт ядра ниже
// перетягивает `vdom::to_json` и всё остальное; здесь добавлены то, что glob
// не даёт: `serde_json` — путь `::light_fw::serde_json` для сгенерированного
// макросом кода (доступен теперь в любом наборе фич), и `devtools` — только
// при фиче.
#[cfg(feature = "devtools")]
pub use light_fw_core::devtools;
pub use light_fw_core::*;
pub use serde_json;

#[cfg(feature = "async-await")]
pub use light_fw_core::spawn_async;
#[cfg(feature = "async-runtime")]
pub use light_fw_core::{
    TaskContext, TaskError, TaskHandle, UserEvent, post_to_ui, post_update, post_update_to, spawn,
};
#[cfg(feature = "async-runtime")]
pub use std::any::Any;

#[cfg(feature = "validation")]
pub use light_fw_behavior_validation;
#[cfg(feature = "devtools")]
pub use light_fw_core::enable_devtools;
#[cfg(feature = "elements-form")]
pub use light_fw_element_form;

/// Профиль из `light.config.json`: один из разделов `dev` / `prod`.
#[cfg(feature = "devtools")]
#[derive(Debug, Clone, Deserialize)]
struct ConfigProfile {
    /// Настройки DevTools (TCP-инспектор).
    #[serde(default)]
    devtools: Option<DevtoolsConfig>,
}

/// Настройки DevTools в `light.config.json`: `enabled` и `port`.
#[cfg(feature = "devtools")]
#[derive(Debug, Clone, Deserialize)]
struct DevtoolsConfig {
    /// Включён ли TCP-сервер инспектора.
    #[serde(default)]
    enabled: bool,
    /// Порт для TCP-сервера (по умолчанию 9229).
    #[serde(default = "default_devtools_port")]
    port: u16,
}

/// Порт DevTools по умолчанию, если он не задан в конфиге.
#[cfg(feature = "devtools")]
fn default_devtools_port() -> u16 {
    9229
}

/// Считывает `light.config.json` и возвращает профиль для текущей сборки.
///
/// При `cfg!(debug_assertions)` (сборка debug) берётся раздел `dev`, иначе —
/// `prod`. Если файла нет или он не парсится, используется профиль с
/// разумными значениями по умолчанию (`devtools` выключен, порт 9229).
#[cfg(feature = "devtools")]
fn load_profile(config_path: &str) -> ConfigProfile {
    let text = match std::fs::read_to_string(config_path) {
        Ok(text) => text,
        Err(_) => return ConfigProfile { devtools: None },
    };
    let parsed: serde_json::Value = match serde_json::from_str(&text) {
        Ok(value) => value,
        Err(_) => return ConfigProfile { devtools: None },
    };
    let section = if cfg!(debug_assertions) {
        "dev"
    } else {
        "prod"
    };
    match parsed.get(section) {
        Some(profile) => {
            serde_json::from_value(profile.clone()).unwrap_or(ConfigProfile { devtools: None })
        }
        None => ConfigProfile { devtools: None },
    }
}

/// Запускает окно приложения с выбором рендер-бэкенда по фиче сборки.
///
/// Это основной публичный API: вызывается из `fn main` пользователя. Бэкенд
/// выбирается по фичам `softbuffer-backend` / `wgpu-backend` (при обоих —
/// используется softbuffer, см. [`light_fw_core::run`]); фабрика бэкенда
/// строится здесь и передаётся в [`light_gui::run_with_backend`].
///
/// 0.20.0 (Этап 5): рендер-крейты (`light-render-cpu` / `light-render-gpu`)
/// вынесены из `light-gui`; фасад `light-fw` теперь сам выбирает бэкенд и
/// передаёт готовую фабрику. Публичные API `light-core` не меняются.
///
/// Пример:
/// ```ignore
/// use light_fw::prelude::*;
///
/// #[template("templates/doc_example.html")]
/// #[proxy]
/// struct App { count: i64 }
/// impl App { fn increment(&self) { self.set_count(self.count() + 1); } }
///
/// fn main() -> light_fw::Result<()> {
///     light_fw::run(App::new())
/// }
/// ```
#[cfg(feature = "window")]
pub fn run(component: impl Component + 'static) -> crate::Result<()> {
    use std::sync::Arc;
    type Win = light_fw_core::window::Window;
    // Фабрика: (width, height, window) -> Box<dyn RendererBackend>.
    // Вызывается при первом показе окна (см. `light_gui::window::windows::App::paint`).
    // `light_gui` — реэкспорт из `light_fw_core` (0.20.0, Этап 5); не
    // добавляем прямую зависимость в `[dependencies]`.
    let factory: Box<
        dyn Fn(u32, u32, Arc<Win>) -> light_gui::Result<Box<dyn light_gui::render::RendererBackend>>
            + Send
            + Sync,
    > = {
        #[cfg(feature = "softbuffer-backend")]
        {
            Box::new(|width: u32, height: u32, window: Arc<Win>| {
                Ok(Box::new(light_render_cpu::SoftbufferBackend::new(
                    width, height, window,
                )?)
                    as Box<dyn light_gui::render::RendererBackend>)
            })
        }
        #[cfg(all(feature = "wgpu-backend", not(feature = "softbuffer-backend")))]
        {
            Box::new(|width: u32, height: u32, window: Arc<Win>| {
                Ok(
                    Box::new(light_render_gpu::WgpuBackend::new(width, height, window)?)
                        as Box<dyn light_gui::render::RendererBackend>,
                )
            })
        }
        #[cfg(all(not(feature = "softbuffer-backend"), not(feature = "wgpu-backend")))]
        {
            Box::new(|width: u32, height: u32, _window: Arc<Win>| {
                Ok(
                    Box::new(light_gui::render::null::NullBackend::new(width, height))
                        as Box<dyn light_gui::render::RendererBackend>,
                )
            })
        }
    };
    light_gui::run_with_backend(component, factory).map_err(crate::error::Error::from)
}

/// Запускает приложение по конфигурации из `light.config.json`.
///
/// Читает `config_path` (при отсутствии файла или ошибке разборки — использует
/// значения по умолчанию). Для отладочной сборки (`cfg!(debug_assertions)`)
/// выбирается раздел `dev`, для release — `prod`; если в выбранном профиле
/// включён DevTools **и фича `devtools` активна** (`cargo dev`), поднимается
/// TCP-сервер на заданном порту (по умолчанию 9229). Затем запускает
/// приложение через [`light_fw_core::run`].
///
/// DevTools стартует только если фича `devtools` включена (`cargo dev`).
/// Без неё сервер не поднимается, даже если в JSON `enabled: true`.
///
/// Пример:
/// ```ignore
/// use light_fw::prelude::*;
///
/// #[template("templates/doc_example.html")]
/// #[proxy]
/// struct App {
///     count: i64,
/// }
///
/// impl App {
///     fn increment(&self) {
///         self.set_count(self.count() + 1);
///     }
/// }
///
/// fn main() -> light_fw::Result<()> {
///     light_fw::run_with_config("light.config.json", App::new())
/// }
/// ```
#[cfg(feature = "window")]
pub fn run_with_config(
    config_path: &str,
    component: impl Component + 'static,
) -> crate::Result<()> {
    let _ = config_path;
    #[cfg(feature = "devtools")]
    {
        if light_fw_core::RUNTIME_TEMPLATES_ENABLED {
            eprintln!("light: runtime-templates enabled (hot HTML reload active)");
        } else {
            eprintln!("light: runtime-templates DISABLED (hot HTML reload will not work)");
        }
    }
    #[cfg(feature = "devtools")]
    {
        let profile = load_profile(config_path);
        if let Some(devtools) = &profile.devtools
            && devtools.enabled
        {
            match enable_devtools(devtools.port) {
                Ok(port) => eprintln!("DevTools listening on 127.0.0.1:{port}"),
                Err(error) => eprintln!(
                    "Не удалось запустить сервер DevTools на порту {}: {error}",
                    devtools.port
                ),
            }
        }
    }
    light_fw_core::run(component)
}
