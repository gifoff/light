//! # relay-element-form
//!
//! Компонент формы: включает элементы `<checkbox>`, `<radio>`, `<select>` и
//! `<option>` в [`relay-core`](relay_core).
//!
//! Реализация живёт в `relay-core` и активируется его фичей `elements-form`.
//! Этот крейт существует ради явной зависимости: приложение, которому нужны
//! элементы формы, добавляет `relay-element-form` (или включает фичу
//! `relay/elements-form`), а сборка без формы не тянет её код.

#![forbid(unsafe_code)]

pub use relay_core::vdom::{NodeKind, SelectOption, VElement};
