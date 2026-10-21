//! # light-fw-behavior-validation
//!
//! Компонент валидации форм: стандартная проверка `required`, пользовательские
//! правила атрибута `:validator` и обработчика `@validate`.
//!
//! Реализация живёт в `light-fw-core` и активируется его фичами `validation` и
//! `required-fields`. Этот крейт существует ради явной зависимости: приложение,
//! которому нужна валидация, добавляет `light-fw-behavior-validation` (или включает
//! фичу `light/validation`), а сборка без валидации не тянет её код.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

pub use light_fw_core::validation::{
    FieldError, ValidationResult, field_value, find_submit_button, form_ancestor, validate_form,
    validate_required,
};
