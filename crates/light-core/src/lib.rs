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
//! ## Пример
//!
//! ```no_run
//! # extern crate light_fw_core as light;
//! use light::prelude::*;
//!
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
//! # #[cfg(not(feature = "window"))]
//! # fn main() {}
//! # #[cfg(feature = "window")]
//! fn main() -> light::Result<()> {
//!     light::run(App::new())
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
//!
//! ## Инструменты разработки
//!
//! Фича `devtools` добавляет модуль [`devtools`] с JSON-снимком состояния
//! приложения: виртуальное дерево, реактивные свойства компонента и (в
//! перспективе) раскладка. Снимок строится функцией
//! [`capture_snapshot`](devtools::capture_snapshot).

#![deny(missing_docs)]
#![forbid(unsafe_code)]

// Позволяет генерируемому коду обращаться к `::light::…` изнутри самой библиотеки.
extern crate self as light;

/// Флаг runtime-templates: доступен ли парсинг HTML-шаблонов во время выполнения.
pub const FEATURE_RUNTIME_TEMPLATES: bool = cfg!(feature = "runtime-templates");

/// Публичная константа для диагностики: включён ли hot HTML reload.
#[cfg(feature = "runtime-templates")]
pub const RUNTIME_TEMPLATES_ENABLED: bool = true;
/// Публичная константа для диагностики: включён ли hot HTML reload.
#[cfg(not(feature = "runtime-templates"))]
pub const RUNTIME_TEMPLATES_ENABLED: bool = false;

// Публичные зависимости, доступные под именем `::light::…` в сгенерированном
// коде (макрос `#[template]`).
#[cfg(feature = "devtools")]
pub use serde_json;

pub mod a11y;
pub mod collection;
pub mod component;
pub mod control;
#[cfg(feature = "devtools")]
pub mod devtools;
pub mod dom;
pub mod error;
#[cfg(feature = "file-dialog")]
pub mod file_dialog;
pub mod layout;
pub mod prelude;
pub mod reactive;
pub mod runtime;
mod text_wrap;
pub mod validation;
pub mod vdom;

#[cfg(feature = "async-runtime")]
pub mod async_runtime;
#[cfg(feature = "window")]
pub mod render;
#[cfg(feature = "window")]
pub mod window;

#[cfg(feature = "async-await")]
pub use async_runtime::spawn_async;
#[cfg(feature = "async-runtime")]
pub use async_runtime::{
    TaskContext, TaskError, TaskHandle, UserEvent, apply_action, post_to_ui, post_update,
    post_update_to, spawn,
};
pub use collection::Collection;
pub use component::{Component, WindowConfig};
pub use control::{
    FocusCommand, WindowCommand, WindowToken, blur, current_window_token, focus, focused_key,
    set_current_window_token,
};
#[cfg(feature = "devtools")]
pub use devtools::{
    DevToolsCommand, DevToolsInspect, DevToolsSnapshot, STATE_FILE, capture_snapshot,
    load_state_file, save_state_file, state_file_path, update_latest_snapshot,
};
pub use dom::Dom;
pub use error::{Error, Result};
pub use light_fw_macros::{proxy, template};
pub use reactive::{Property, Proxy, ProxyState, Watcher, watch};
#[cfg(feature = "window")]
pub use render::RendererBackend;
#[cfg(feature = "wgpu-backend")]
pub use render::WgpuBackend;
#[cfg(feature = "image-support")]
pub use vdom::ImageData;
pub use vdom::{
    Align, Edges, Event, KeyInfo, MouseButton, NodeKind, Offset, Overflow, Position, Resize,
    SelectOption, Size, Style, VElement, VNode, VWindow, WhiteSpace,
};

/// Открывает окно с интерфейсом компонента и запускает цикл событий.
///
/// Возвращает только при закрытии окна или ошибке.
#[cfg(feature = "window")]
pub fn run(component: impl Component + 'static) -> Result<()> {
    window::run(component)
}

/// Запускает TCP-сервер DevTools на `127.0.0.1:port`.
///
/// Возвращает фактический порт (полезно при `port = 0`). Сервер работает в
/// отдельном потоке и не блокирует UI. Подключитесь через TCP и отправьте
/// строку `SNAPSHOT\n` — в ответ придёт JSON с полями `window_token`,
/// `vdom`, `state`, `layout`. Команда `QUIT\n` закрывает соединение.
///
/// Пример:
/// ```no_run
/// # extern crate light_fw_core as light;
/// let port = light::enable_devtools(9229).expect("devtools server started");
/// eprintln!("DevTools listening on 127.0.0.1:{}", port);
/// ```
#[cfg(feature = "devtools")]
pub fn enable_devtools(port: u16) -> std::io::Result<u16> {
    devtools::server::start(port)
}
