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
//! ## Компоненты
//!
//! Ядро библиотеки — крейт [`relay-core`](relay_core). Дополнительные
//! возможности подключаются как отдельные компоненты и фичи:
//!
//! * [`relay-element-form`](relay_element_form) (фича `elements-form`) —
//!   элементы `<checkbox>`, `<radio>`, `<select>`/`<option>`;
//! * [`relay-behavior-validation`](relay_behavior_validation)
//!   (фича `validation`) — `required`, `:validator` и `@validate`.
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

pub use relay_core::*;

#[cfg(feature = "validation")]
pub use relay_behavior_validation;
#[cfg(feature = "elements-form")]
pub use relay_element_form;
