//! Валидация форм: стандартная проверка `required` и пользовательские правила.
//!
//! Модуль не зависит от окна: он работает с готовым деревом [`VElement`] и
//! вызывает пользовательские проверки через методы
//! [`Component::validate_field`] и [`Component::validate_form`], которые
//! генерирует макрос `#[template]`.

use crate::component::Component;
use crate::vdom::{ButtonType, NodeKind, VElement};

/// Сообщение для пустого обязательного текстового поля.
pub const REQUIRED_TEXT: &str = "Заполните это поле";
/// Сообщение для пустого обязательного многострочного поля.
pub const REQUIRED_TEXTAREA: &str = "Введите текст";
/// Сообщение для неотмеченного обязательного флажка.
pub const REQUIRED_CHECK: &str = "Отметьте этот пункт";
/// Сообщение для группы обязательных переключателей без выбора.
pub const REQUIRED_RADIO: &str = "Выберите один из вариантов";
/// Сообщение для невыбранного обязательного списка.
pub const REQUIRED_SELECT: &str = "Выберите значение";

/// Ошибка проверки одного поля.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldError {
    /// Ключ невалидного узла.
    pub key: String,
    /// Сообщение для пользователя.
    pub message: String,
}

/// Результат проверки формы.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ValidationResult {
    /// Найденные ошибки в порядке обхода дерева.
    pub errors: Vec<FieldError>,
}

impl ValidationResult {
    /// Валидна ли форма (нет ошибок).
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }

    /// Первое невалидное поле, если оно есть.
    pub fn first(&self) -> Option<&FieldError> {
        self.errors.first()
    }

    /// Сообщение об ошибке для поля с ключом `key`.
    pub fn message_for(&self, key: &str) -> Option<&str> {
        self.errors
            .iter()
            .find(|error| error.key == key)
            .map(|error| error.message.as_str())
    }
}

/// Строковое значение поля для пользовательского валидатора.
///
/// Для флажка `v-model` приводится к `"true"`/`"false"`, для переключателя —
/// только выбранный (`selected`) отдаёт своё `value`, иначе пустая строка.
pub fn field_value(node: &VElement) -> String {
    match &node.kind {
        NodeKind::TextField { value, .. } => value.clone(),
        NodeKind::Checkbox { checked, .. } => checked.to_string(),
        NodeKind::Radio {
            selected, value, ..
        } => {
            if *selected {
                value.clone()
            } else {
                String::new()
            }
        }
        NodeKind::Select { value, .. } => value.clone(),
        _ => String::new(),
    }
}

/// Стандартная проверка обязательных полей поддерева `root`.
///
/// Группа обязательных `<radio>` (прямые дети одного контейнера) считается
/// заполненной, если выбран хотя бы один переключатель.
pub fn validate_required(root: &VElement) -> ValidationResult {
    let mut errors = Vec::new();
    check_required(root, &mut errors);
    ValidationResult { errors }
}

/// Проверяет форму: пользовательская проверка `@validate` имеет приоритет,
/// иначе выполняется стандартная проверка `required` и `:validator` полей.
pub fn validate_form(
    root: &VElement,
    component: &dyn Component,
    form_key: &str,
) -> ValidationResult {
    if let Some(errors) = component.validate_form(form_key) {
        return ValidationResult {
            errors: errors
                .into_iter()
                .map(|(key, message)| FieldError { key, message })
                .collect(),
        };
    }
    let mut result = validate_required(root);
    apply_field_validators(root, component, &mut result);
    result
}

/// Ближайший предок узла `key` с `role="form"` (сам узел не учитывается).
pub fn form_ancestor<'a>(root: &'a VElement, key: &str) -> Option<&'a VElement> {
    fn walk<'a>(
        node: &'a VElement,
        key: &str,
        forms: &mut Vec<&'a VElement>,
    ) -> Option<&'a VElement> {
        if node.key == key {
            return forms.last().copied();
        }
        let is_form = node
            .attr("role")
            .is_some_and(|role| role.eq_ignore_ascii_case("form"));
        if is_form {
            forms.push(node);
        }
        for child in &node.children {
            if let Some(found) = walk(child, key, forms) {
                return Some(found);
            }
        }
        if is_form {
            forms.pop();
        }
        None
    }
    walk(root, key, &mut Vec::new())
}

/// Первая кнопка отправки формы, содержащей поле `focused_key`.
///
/// Формой считается ближайший предок поля с `role="form"`. Возвращается первая
/// по порядку обхода кнопка с `type="submit"` (или без явного `type`), которая
/// не отключена. Кнопки `type="button"`/`type="reset"` и `disabled` не
/// считаются кнопкой отправки.
pub fn find_submit_button<'a>(root: &'a VElement, focused_key: &str) -> Option<&'a VElement> {
    fn first_submit(node: &VElement) -> Option<&VElement> {
        if matches!(
            node.kind,
            NodeKind::Button {
                disabled: false,
                button_type: ButtonType::Submit,
                ..
            }
        ) {
            return Some(node);
        }
        node.children.iter().find_map(first_submit)
    }
    first_submit(form_ancestor(root, focused_key)?)
}

fn check_required(node: &VElement, errors: &mut Vec<FieldError>) {
    let required_radios: Vec<&VElement> = node
        .children
        .iter()
        .filter(|child| child.style.required && matches!(child.kind, NodeKind::Radio { .. }))
        .collect();
    if !required_radios.is_empty() {
        let any_selected = node
            .children
            .iter()
            .any(|child| matches!(child.kind, NodeKind::Radio { selected: true, .. }));
        if !any_selected {
            errors.push(FieldError {
                key: required_radios[0].key.to_owned(),
                message: REQUIRED_RADIO.to_owned(),
            });
        }
    }

    if node.style.required {
        let message = match &node.kind {
            NodeKind::TextField {
                value, multiline, ..
            } => {
                if value.trim().is_empty() {
                    Some(if *multiline {
                        REQUIRED_TEXTAREA
                    } else {
                        REQUIRED_TEXT
                    })
                } else {
                    None
                }
            }
            NodeKind::Checkbox { checked, .. } => (!*checked).then_some(REQUIRED_CHECK),
            NodeKind::Select { value, .. } => value.is_empty().then_some(REQUIRED_SELECT),
            _ => None,
        };
        if let Some(message) = message {
            errors.push(FieldError {
                key: node.key.to_owned(),
                message: message.to_owned(),
            });
        }
    }

    for child in &node.children {
        check_required(child, errors);
    }
}

fn apply_field_validators(
    node: &VElement,
    component: &dyn Component,
    result: &mut ValidationResult,
) {
    if matches!(
        node.kind,
        NodeKind::TextField { .. }
            | NodeKind::Checkbox { .. }
            | NodeKind::Radio { .. }
            | NodeKind::Select { .. }
    ) && result.message_for(node.key).is_none()
        && let Err(message) = component.validate_field(node.key, &field_value(node))
    {
        result.errors.push(FieldError {
            key: node.key.to_owned(),
            message,
        });
    }

    for child in &node.children {
        apply_field_validators(child, component, result);
    }
}
