//! # light-fw-element-form
//!
//! Компонент формы: включает элементы `<checkbox>`, `<radio>`, `<select>` и
//! `<option>` в [`light-fw-core`](light_fw_core).
//!
//! Реализация живёт в `light-fw-core` и активируется его фичей `elements-form`.
//! Этот крейт существует ради явной зависимости: приложение, которому нужны
//! элементы формы, добавляет `light-fw-element-form` (или включает фичу
//! `light/elements-form`), а сборка без формы не тянет её код.

#![forbid(unsafe_code)]

pub use light_fw_core::vdom::{NodeKind, SelectOption, VElement};
