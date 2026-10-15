//! Валидация форм: стандартный `required`, пользовательские `:validator`
//! и `@validate`, а также атрибуты `required`/`type` в разметке.

#![cfg(all(
    feature = "elements-checkbox",
    feature = "elements-radio",
    feature = "elements-select"
))]

use relay::prelude::*;
use relay::validation::{self, REQUIRED_CHECK, REQUIRED_SELECT, REQUIRED_TEXT};
use relay::vdom::{ButtonType, NodeKind, VElement};

/// Анкета с обязательными полями и пользовательскими проверками.
#[template("tests/templates/validation.html")]
#[proxy]
struct FormApp {
    name: String,
    email: String,
    agree: bool,
    city: String,
}

impl FormApp {
    fn check_name(&self, value: &str) -> Result<(), String> {
        if value.chars().count() < 3 {
            Err("Слишком короткое имя".to_owned())
        } else {
            Ok(())
        }
    }

    fn check_email(&self, value: &str) -> Result<(), String> {
        if value.contains('@') {
            Ok(())
        } else {
            Err("Нужен символ @".to_owned())
        }
    }
}

/// Форма с собственным `@validate`, всегда считающая себя валидной.
#[template("tests/templates/validation_custom.html")]
#[proxy]
struct PermissiveForm {
    name: String,
}

impl PermissiveForm {
    fn collect_errors(&self) -> Result<(), Vec<(String, String)>> {
        Ok(())
    }
}

/// Форма с собственным `@validate`, возвращающая заранее заданную ошибку.
#[template("tests/templates/validation_custom.html")]
#[proxy]
struct StrictForm {
    name: String,
}

impl StrictForm {
    fn collect_errors(&self) -> Result<(), Vec<(String, String)>> {
        Err(vec![("0.0".to_owned(), "Заполните по-своему".to_owned())])
    }
}

fn field<'a>(tree: &'a VElement, placeholder: &str) -> &'a VElement {
    tree.find_by_attr("placeholder", placeholder)
        .unwrap_or_else(|| panic!("поле `{placeholder}` не найдено"))
}

fn submit_button(tree: &VElement) -> &VElement {
    find_button(tree, "Отправить")
}

/// Ищет кнопку-контейнер по тексту её прямого текстового ребёнка.
fn find_button<'a>(tree: &'a VElement, label: &str) -> &'a VElement {
    tree.descendants()
        .into_iter()
        .find(|node| {
            matches!(node.kind, NodeKind::Button { .. })
                && node.children.iter().any(
                    |child| matches!(&child.kind, NodeKind::Text { content } if content == label),
                )
        })
        .unwrap_or_else(|| panic!("кнопка `{label}` не найдена"))
}

#[test]
fn required_errors_on_empty_form() {
    let app = FormApp::new();
    let tree = app.render();
    let form = validation::form_ancestor(&tree, submit_button(&tree).key).expect("форма найдена");

    let result = validation::validate_form(form, &app, form.key);

    assert!(!result.is_valid());
    assert_eq!(result.errors.len(), 4, "имя, почта, флажок и список");
    assert_eq!(
        result.first().map(|error| error.message.as_str()),
        Some(REQUIRED_TEXT)
    );
    assert_eq!(
        result.message_for(field(&tree, "Имя").key),
        Some(REQUIRED_TEXT)
    );
    assert_eq!(
        result.message_for(tree.find_by_tag("checkbox").expect("флажок").key),
        Some(REQUIRED_CHECK)
    );
    assert_eq!(
        result.message_for(tree.find_by_tag("select").expect("список").key),
        Some(REQUIRED_SELECT)
    );
}

#[test]
fn custom_field_validators_run_for_filled_fields() {
    let app = FormApp::new();
    app.set_name("Аб".to_owned());
    app.set_email("почта".to_owned());
    app.set_agree(true);
    app.set_city("msk".to_owned());

    let tree = app.render();
    let submit = submit_button(&tree);
    let form = validation::form_ancestor(&tree, submit.key).expect("форма");
    let result = validation::validate_form(form, &app, form.key);

    assert_eq!(result.errors.len(), 2);
    assert_eq!(
        result.message_for(field(&tree, "Имя").key),
        Some("Слишком короткое имя")
    );
    assert_eq!(
        result.message_for(field(&tree, "Почта").key),
        Some("Нужен символ @")
    );
}

#[test]
fn form_is_valid_when_all_fields_satisfied() {
    let app = FormApp::new();
    app.set_name("Анна".to_owned());
    app.set_email("anna@example.com".to_owned());
    app.set_agree(true);
    app.set_city("msk".to_owned());

    let tree = app.render();
    let form = validation::form_ancestor(&tree, submit_button(&tree).key).expect("форма");
    let result = validation::validate_form(form, &app, form.key);

    assert!(result.is_valid(), "ошибки: {:?}", result.errors);
}

#[test]
fn required_errors_use_expected_messages() {
    let app = FormApp::new();
    let tree = app.render();
    let form = validation::form_ancestor(&tree, submit_button(&tree).key).expect("форма");
    let result = validation::validate_form(form, &app, form.key);

    let messages: Vec<&str> = result.errors.iter().map(|e| e.message.as_str()).collect();
    assert!(messages.contains(&REQUIRED_TEXT));
    assert!(messages.contains(&REQUIRED_CHECK));
    assert!(messages.contains(&REQUIRED_SELECT));
    assert_eq!(result.first().map(|e| e.key.as_str()), Some("0.0"));
}

#[test]
fn form_ancestor_is_none_outside_form() {
    let app = FormApp::new();
    let tree = app.render();
    let form = validation::form_ancestor(&tree, submit_button(&tree).key).expect("форма");
    assert_eq!(form.attr("role"), Some("form"));
    assert!(
        validation::form_ancestor(&tree, form.key).is_none(),
        "форма не вложена сама в себя"
    );
}

#[test]
fn submit_and_plain_button_types_are_parsed() {
    let app = FormApp::new();
    let tree = app.render();

    let submit = submit_button(&tree);
    assert!(matches!(
        submit.kind,
        NodeKind::Button {
            button_type: ButtonType::Submit,
            ..
        }
    ));

    let cancel = find_button(&tree, "Отмена");
    assert!(matches!(
        cancel.kind,
        NodeKind::Button {
            button_type: ButtonType::Button,
            ..
        }
    ));
}

#[test]
fn custom_form_validator_overrides_required() {
    let app = PermissiveForm::new();
    let tree = app.render();
    let form = validation::form_ancestor(&tree, submit_button(&tree).key).expect("форма");

    let result = validation::validate_form(form, &app, form.key);

    assert!(
        result.is_valid(),
        "собственный @validate переопределяет required"
    );
}

#[test]
fn custom_form_validator_errors_are_returned() {
    let app = StrictForm::new();
    let tree = app.render();
    let form = validation::form_ancestor(&tree, submit_button(&tree).key).expect("форма");
    let result = validation::validate_form(form, &app, form.key);

    assert_eq!(result.errors.len(), 1);
    assert_eq!(result.errors[0].key, "0.0");
    assert_eq!(result.errors[0].message, "Заполните по-своему");
}

#[test]
fn required_fields_carry_required_style() {
    let app = FormApp::new();
    let tree = app.render();
    assert!(field(&tree, "Имя").style.required);
    assert!(field(&tree, "Почта").style.required);
    let checkbox = tree.find_by_tag("checkbox").expect("флажок");
    assert!(checkbox.style.required);
    assert!(tree.find_by_tag("select").expect("список").style.required);
}
