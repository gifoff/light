//! # Relay — реактивный графический интерфейс для Rust
//!
//! Relay — библиотека со **своим окном** (winit + программная отрисовка)
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
//! use relay::prelude::*;
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
//! fn main() -> relay::Result<()> {
//!     relay::run(App::new())
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

#![forbid(unsafe_code)]

// Позволяет генерируемому коду обращаться к `::relay::…` изнутри самой библиотеки.
extern crate self as relay;

pub mod a11y;
pub mod collection;
pub mod component;
pub mod control;
pub mod dom;
pub mod error;
pub mod layout;
pub mod prelude;
pub mod reactive;
pub mod validation;
pub mod vdom;

#[cfg(feature = "window")]
pub mod window;

pub use collection::Collection;
pub use component::{Component, WindowConfig};
pub use control::{FocusCommand, WindowCommand, blur, focus, focused_key};
pub use dom::Dom;
pub use error::{Error, Result};
pub use reactive::{Property, Proxy, ProxyState, Watcher, watch};
pub use relay_macros::{proxy, template};
pub use vdom::{
    Align, Edges, Event, KeyInfo, MouseButton, NodeKind, Offset, Resize, SelectOption, Size, Style,
    VElement, VNode, VWindow,
};

/// Открывает окно с интерфейсом компонента и запускает цикл событий.
///
/// Возвращает только при закрытии окна или ошибке.
#[cfg(feature = "window")]
pub fn run(component: impl Component + 'static) -> Result<()> {
    window::run(component)
}
