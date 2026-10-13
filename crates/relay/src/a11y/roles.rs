//! Сопоставление HTML-подобных тегов Relay и атрибутов с ролями AccessKit.
//!
//! Роль узла определяется автоматически по имени тега (или по типу
//! [`NodeKind`]), но её можно переопределить атрибутом `role="…"`, значения
//! которого записываются в kebab-case (`role="list-item"`, `role="tab"`).

use accesskit::{Role, Toggled};

use crate::vdom::{NodeKind, VElement};

/// Роль по значению атрибута `role="…"` (kebab-case).
///
/// Возвращает `None`, если значение неизвестно.
pub fn role_from_name(name: &str) -> Option<Role> {
    let name = name.trim().to_ascii_lowercase();
    let role = match name.as_str() {
        "window" => Role::Window,
        "titlebar" | "title-bar" => Role::TitleBar,
        "button" => Role::Button,
        "checkbox" | "check-box" => Role::CheckBox,
        "radio" | "radio-button" => Role::RadioButton,
        "radiogroup" | "radio-group" => Role::RadioGroup,
        "textbox" | "text-input" | "text" => Role::TextInput,
        "multiline-textbox" | "textarea" => Role::MultilineTextInput,
        "password" | "password-input" => Role::PasswordInput,
        "combobox" | "combo-box" | "select" => Role::ComboBox,
        "listbox" | "list-box" => Role::ListBox,
        "listitem" | "list-item" => Role::ListItem,
        "tab" => Role::Tab,
        "tabpanel" | "tab-panel" => Role::TabPanel,
        "switch" => Role::Switch,
        "link" => Role::Link,
        "slider" => Role::Slider,
        "spinbutton" | "spin-button" => Role::SpinButton,
        "progressbar" | "progress-bar" | "progress-indicator" => Role::ProgressIndicator,
        "menuitem" | "menu-item" => Role::MenuItem,
        "treeitem" | "tree-item" => Role::TreeItem,
        "heading" => Role::Heading,
        "paragraph" => Role::Paragraph,
        "label" => Role::Label,
        "image" | "img" => Role::Image,
        "group" => Role::Group,
        "region" => Role::Region,
        "alert" => Role::Alert,
        "dialog" => Role::Dialog,
        "alertdialog" | "alert-dialog" => Role::AlertDialog,
        "tooltip" => Role::Tooltip,
        "status" => Role::Status,
        "note" => Role::Note,
        "form" => Role::Form,
        "figure" => Role::Figure,
        "term" => Role::Term,
        "definition" => Role::Definition,
        "article" => Role::Article,
        "main" => Role::Main,
        "navigation" => Role::Navigation,
        "banner" => Role::Banner,
        "contentinfo" | "content-info" => Role::ContentInfo,
        "complementary" => Role::Complementary,
        "presentation" | "none" | "generic" | "generic-container" => Role::GenericContainer,
        _ => return None,
    };
    Some(role)
}

/// Уровень заголовка для тегов `h1`…`h5`.
pub fn heading_level(tag: &str) -> Option<usize> {
    match tag {
        "h1" => Some(1),
        "h2" => Some(2),
        "h3" => Some(3),
        "h4" => Some(4),
        "h5" => Some(5),
        _ => None,
    }
}

/// Роль, соответствующая тегу, если она не зависит от типа узла.
///
/// Используется как запасной вариант для тегов, не имеющих собственного
/// варианта [`NodeKind`] (например, заголовков).
pub fn role_for_tag(tag: &str) -> Option<Role> {
    match tag {
        "window" => Some(Role::Window),
        "titlebar" => Some(Role::TitleBar),
        "div" | "span" => Some(Role::GenericContainer),
        "img" | "image" => Some(Role::Image),
        "a" | "link" => Some(Role::Link),
        _ if heading_level(tag).is_some() => Some(Role::Heading),
        _ => None,
    }
}

/// Роль по умолчанию, выведенная из типа узла.
pub fn default_role(kind: &NodeKind) -> Role {
    match kind {
        NodeKind::Window => Role::Window,
        NodeKind::TitleBar => Role::TitleBar,
        NodeKind::Column | NodeKind::Row => Role::GenericContainer,
        NodeKind::Text { .. } => Role::Label,
        NodeKind::Button { .. } => Role::Button,
        NodeKind::TextField { masked: true, .. } => Role::PasswordInput,
        NodeKind::TextField {
            multiline: true, ..
        } => Role::MultilineTextInput,
        NodeKind::TextField { .. } => Role::TextInput,
        NodeKind::Checkbox { .. } => Role::CheckBox,
        NodeKind::Radio { .. } => Role::RadioButton,
        NodeKind::Select { .. } => Role::ComboBox,
    }
}

/// Итоговая роль узла: явное `role="…"` важнее роли по умолчанию.
pub fn role_for(element: &VElement) -> Role {
    if let Some(name) = element.attr("role")
        && let Some(role) = role_from_name(name)
    {
        return role;
    }
    role_for_tag(element.tag).unwrap_or_else(|| default_role(&element.kind))
}

/// Фокусируема ли роль по умолчанию (попадает в обход клавишей `Tab`).
pub fn is_focusable_by_role(role: Role) -> bool {
    matches!(
        role,
        Role::Button
            | Role::CheckBox
            | Role::RadioButton
            | Role::TextInput
            | Role::MultilineTextInput
            | Role::PasswordInput
            | Role::ComboBox
            | Role::Link
            | Role::Slider
            | Role::SpinButton
            | Role::ListBoxOption
            | Role::MenuItem
            | Role::MenuItemCheckBox
            | Role::MenuItemRadio
            | Role::Tab
            | Role::Switch
            | Role::ListBox
            | Role::TreeItem
    )
}

/// Разбирает булев атрибут (`true`/`false`; пустое значение — `true`).
pub use super::parse_bool;

/// Разбирает трёхзначный `aria-checked` (`true`/`false`/`mixed`).
pub fn parse_tristate(value: &str) -> Option<Toggled> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" => Some(Toggled::True),
        "false" => Some(Toggled::False),
        "mixed" => Some(Toggled::Mixed),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vdom::Style;

    fn element(tag: &'static str, kind: NodeKind, attrs: &[(&'static str, &str)]) -> VElement {
        let mut node = VElement::leaf("k", kind).with_tag(tag);
        node.style = Style::default();
        for (name, value) in attrs {
            node.attrs.push((name, (*value).to_owned()));
        }
        node
    }

    #[test]
    fn tags_map_to_expected_roles() {
        assert_eq!(role_for_tag("div"), Some(Role::GenericContainer));
        assert_eq!(role_for_tag("span"), Some(Role::GenericContainer));
        assert_eq!(role_for_tag("window"), Some(Role::Window));
        assert_eq!(role_for_tag("h3"), Some(Role::Heading));
        assert_eq!(role_for_tag("unknown-tag"), None);
    }

    #[test]
    fn heading_levels_are_extracted() {
        assert_eq!(heading_level("h1"), Some(1));
        assert_eq!(heading_level("h5"), Some(5));
        assert_eq!(heading_level("h6"), None);
    }

    #[test]
    fn kind_defaults_cover_form_controls() {
        assert_eq!(
            default_role(&NodeKind::Checkbox {
                label: String::new(),
                checked: false,
                disabled: false,
            }),
            Role::CheckBox
        );
        assert_eq!(
            default_role(&NodeKind::Select {
                value: String::new(),
                options: Vec::new(),
                disabled: false,
            }),
            Role::ComboBox
        );
        assert_eq!(
            default_role(&NodeKind::TextField {
                value: String::new(),
                placeholder: String::new(),
                disabled: false,
                multiline: true,
                resize: crate::vdom::Resize::None,
                masked: false,
            }),
            Role::MultilineTextInput
        );
        assert_eq!(
            default_role(&NodeKind::TextField {
                value: String::new(),
                placeholder: String::new(),
                disabled: false,
                multiline: false,
                resize: crate::vdom::Resize::None,
                masked: true,
            }),
            Role::PasswordInput
        );
    }

    #[test]
    fn explicit_role_overrides_tag() {
        let node = element("div", NodeKind::Column, &[("role", "list-item")]);
        assert_eq!(role_for(&node), Role::ListItem);
    }

    #[test]
    fn unknown_role_falls_back_to_tag() {
        let node = element("h2", NodeKind::Column, &[("role", "not-a-role")]);
        assert_eq!(role_for(&node), Role::Heading);
    }

    #[test]
    fn role_from_name_accepts_kebab_case() {
        assert_eq!(role_from_name("list-item"), Some(Role::ListItem));
        assert_eq!(role_from_name("tab"), Some(Role::Tab));
        assert_eq!(role_from_name("radio-group"), Some(Role::RadioGroup));
        assert_eq!(role_from_name("nope"), None);
    }

    #[test]
    fn focusable_roles_are_recognised() {
        assert!(is_focusable_by_role(Role::Button));
        assert!(is_focusable_by_role(Role::ComboBox));
        assert!(!is_focusable_by_role(Role::GenericContainer));
        assert!(!is_focusable_by_role(Role::Heading));
    }

    #[test]
    fn boolean_and_tristate_parsing() {
        assert_eq!(parse_bool("true"), Some(true));
        assert_eq!(parse_bool(""), Some(true));
        assert_eq!(parse_bool("FALSE"), Some(false));
        assert_eq!(parse_bool("maybe"), None);
        assert_eq!(parse_tristate("mixed"), Some(Toggled::Mixed));
        assert_eq!(parse_tristate("true"), Some(Toggled::True));
        assert_eq!(parse_tristate("false"), Some(Toggled::False));
        assert_eq!(parse_tristate("TRUE"), Some(Toggled::True));
        assert_eq!(parse_tristate("loud"), None);
    }

    #[test]
    fn role_from_name_covers_all_mappings() {
        let cases = [
            ("window", Role::Window),
            ("titlebar", Role::TitleBar),
            ("title-bar", Role::TitleBar),
            ("button", Role::Button),
            ("checkbox", Role::CheckBox),
            ("check-box", Role::CheckBox),
            ("radio", Role::RadioButton),
            ("radio-group", Role::RadioGroup),
            ("textbox", Role::TextInput),
            ("text-input", Role::TextInput),
            ("textarea", Role::MultilineTextInput),
            ("password", Role::PasswordInput),
            ("select", Role::ComboBox),
            ("listbox", Role::ListBox),
            ("list-item", Role::ListItem),
            ("listitem", Role::ListItem),
            ("tab", Role::Tab),
            ("tabpanel", Role::TabPanel),
            ("switch", Role::Switch),
            ("link", Role::Link),
            ("slider", Role::Slider),
            ("spinbutton", Role::SpinButton),
            ("progressbar", Role::ProgressIndicator),
            ("menuitem", Role::MenuItem),
            ("treeitem", Role::TreeItem),
            ("heading", Role::Heading),
            ("paragraph", Role::Paragraph),
            ("label", Role::Label),
            ("image", Role::Image),
            ("img", Role::Image),
            ("group", Role::Group),
            ("region", Role::Region),
            ("alert", Role::Alert),
            ("dialog", Role::Dialog),
            ("alertdialog", Role::AlertDialog),
            ("tooltip", Role::Tooltip),
            ("status", Role::Status),
            ("note", Role::Note),
            ("form", Role::Form),
            ("figure", Role::Figure),
            ("term", Role::Term),
            ("definition", Role::Definition),
            ("article", Role::Article),
            ("main", Role::Main),
            ("navigation", Role::Navigation),
            ("banner", Role::Banner),
            ("contentinfo", Role::ContentInfo),
            ("complementary", Role::Complementary),
            ("presentation", Role::GenericContainer),
            ("none", Role::GenericContainer),
            ("generic", Role::GenericContainer),
        ];
        for (name, expected) in cases {
            assert_eq!(role_from_name(name), Some(expected), "роль `{name}`");
        }
    }

    #[test]
    fn unknown_and_empty_names_are_rejected() {
        assert_eq!(role_from_name(""), None);
        assert_eq!(role_from_name("   "), None);
        assert_eq!(role_from_name("not-a-role"), None);
        assert_eq!(role_from_name("heading-2"), None);
    }

    #[test]
    fn role_names_are_case_insensitive() {
        assert_eq!(role_from_name("ListBox"), Some(Role::ListBox));
        assert_eq!(role_from_name("RADIO-GROUP"), Some(Role::RadioGroup));
    }

    #[test]
    fn role_for_prefers_explicit_role_over_kind() {
        let field = element(
            "text",
            NodeKind::TextField {
                value: String::new(),
                placeholder: String::new(),
                disabled: false,
                multiline: false,
                resize: crate::vdom::Resize::None,
                masked: true,
            },
            &[("role", "combobox")],
        );
        assert_eq!(role_for(&field), Role::ComboBox);
    }

    #[test]
    fn role_for_derives_from_kind_without_tag() {
        let masked = element(
            "text",
            NodeKind::TextField {
                value: String::new(),
                placeholder: String::new(),
                disabled: false,
                multiline: false,
                resize: crate::vdom::Resize::None,
                masked: true,
            },
            &[],
        );
        assert_eq!(role_for(&masked), Role::PasswordInput);
    }

    #[test]
    fn whitespace_in_role_name_is_trimmed() {
        let node = element("div", NodeKind::Column, &[("role", "  tab  ")]);
        assert_eq!(role_for(&node), Role::Tab);
    }

    #[test]
    fn non_focusable_roles_are_rejected() {
        for role in [
            Role::GenericContainer,
            Role::Heading,
            Role::Paragraph,
            Role::Label,
            Role::Image,
            Role::Region,
            Role::ListItem,
            Role::Alert,
            Role::Dialog,
            Role::Window,
            Role::TitleBar,
        ] {
            assert!(
                !is_focusable_by_role(role),
                "роль {role:?} не должна быть фокусируемой"
            );
        }
    }
}
