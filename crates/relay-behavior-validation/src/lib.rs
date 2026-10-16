//! # relay-behavior-validation
//!
//! Компонент валидации форм: стандартная проверка `required`, пользовательские
//! правила атрибута `:validator` и обработчика `@validate`.
//!
//! Реализация живёт в `relay-core` и активируется его фичами `validation` и
//! `required-fields`. Этот крейт существует ради явной зависимости: приложение,
//! которому нужна валидация, добавляет `relay-behavior-validation` (или включает
//! фичу `relay/validation`), а сборка без валидации не тянет её код.

#![forbid(unsafe_code)]

pub use relay_core::validation::{
    FieldError, ValidationResult, field_value, find_submit_button, form_ancestor, validate_form,
    validate_required,
};
