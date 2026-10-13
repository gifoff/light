//! Тесты сопоставления тегов, имён ролей и разбора значений атрибутов.
#![cfg(feature = "a11y")]

use accesskit::{Role, Toggled};
use relay::a11y::roles::{
    default_role, heading_level, parse_bool, parse_tristate, role_for_tag, role_from_name,
};
use relay::vdom::NodeKind;

// ---------- role_for_tag: только структурные теги ----------

#[test]
fn structural_tags_map_to_roles() {
    assert_eq!(role_for_tag("window"), Some(Role::Window));
    assert_eq!(role_for_tag("titlebar"), Some(Role::TitleBar));
    assert_eq!(role_for_tag("div"), Some(Role::GenericContainer));
    assert_eq!(role_for_tag("span"), Some(Role::GenericContainer));
    assert_eq!(role_for_tag("img"), Some(Role::Image));
    assert_eq!(role_for_tag("image"), Some(Role::Image));
    assert_eq!(role_for_tag("a"), Some(Role::Link));
    assert_eq!(role_for_tag("link"), Some(Role::Link));
}

#[test]
fn heading_tags_map_to_heading_role() {
    for tag in ["h1", "h2", "h3", "h4", "h5"] {
        assert_eq!(role_for_tag(tag), Some(Role::Heading), "tag = {tag}");
    }
}

#[test]
fn heading_level_matches_number() {
    assert_eq!(heading_level("h1"), Some(1));
    assert_eq!(heading_level("h2"), Some(2));
    assert_eq!(heading_level("h3"), Some(3));
    assert_eq!(heading_level("h4"), Some(4));
    assert_eq!(heading_level("h5"), Some(5));
    assert_eq!(heading_level("h6"), None);
    assert_eq!(heading_level("div"), None);
}

#[test]
fn unknown_tag_returns_none() {
    // button / checkbox / text / select — это NodeKind, а не теги.
    assert_eq!(role_for_tag("button"), None);
    assert_eq!(role_for_tag("checkbox"), None);
    assert_eq!(role_for_tag("text"), None);
    assert_eq!(role_for_tag("не-тег"), None);
    assert_eq!(role_for_tag(""), None);
}

// ---------- role_from_name ----------

#[test]
fn role_from_name_accepts_kebab_case() {
    assert_eq!(role_from_name("tab"), Some(Role::Tab));
    assert_eq!(role_from_name("list-item"), Some(Role::ListItem));
    assert_eq!(role_from_name("tab-panel"), Some(Role::TabPanel));
    assert_eq!(role_from_name("check-box"), Some(Role::CheckBox));
    assert_eq!(role_from_name("combo-box"), Some(Role::ComboBox));
}

#[test]
fn role_from_name_trims_whitespace() {
    assert_eq!(role_from_name("  tab  "), Some(Role::Tab));
    assert_eq!(role_from_name("\ttab\n"), Some(Role::Tab));
}

#[test]
fn role_from_name_is_case_insensitive() {
    // По WAI-ARIA значения role регистронезависимы.
    assert_eq!(role_from_name("TAB"), Some(Role::Tab));
    assert_eq!(role_from_name("Tab"), Some(Role::Tab));
    assert_eq!(role_from_name("  TAB  "), Some(Role::Tab));
    assert_eq!(role_from_name("List-Item"), Some(Role::ListItem));
}

#[test]
fn unknown_role_returns_none() {
    assert_eq!(role_from_name("не-существует"), None);
    assert_eq!(role_from_name(""), None);
}

// ---------- parse_bool / parse_tristate ----------

#[test]
fn parse_bool_accepts_true_forms() {
    for v in ["true", "True", " TRUE ", "1", "yes", ""] {
        assert_eq!(parse_bool(v), Some(true), "value = {v:?}");
    }
}

#[test]
fn parse_bool_accepts_false_forms() {
    for v in ["false", "False", " FALSE ", "0", "no"] {
        assert_eq!(parse_bool(v), Some(false), "value = {v:?}");
    }
}

#[test]
fn parse_bool_rejects_garbage() {
    assert_eq!(parse_bool("maybe"), None);
    assert_eq!(parse_bool("2"), None);
    assert_eq!(parse_bool("tru"), None);
}

#[test]
fn parse_tristate_supports_mixed() {
    assert_eq!(parse_tristate("true"), Some(Toggled::True));
    assert_eq!(parse_tristate("false"), Some(Toggled::False));
    assert_eq!(parse_tristate("mixed"), Some(Toggled::Mixed));
    assert_eq!(parse_tristate("MIXED"), Some(Toggled::Mixed));
    assert_eq!(parse_tristate("undefined"), None);
}

// ---------- default_role: маппинг NodeKind → Role ----------

#[test]
fn default_role_for_containers() {
    assert_eq!(default_role(&NodeKind::Window), Role::Window);
    assert_eq!(default_role(&NodeKind::TitleBar), Role::TitleBar);
    assert_eq!(default_role(&NodeKind::Column), Role::GenericContainer);
    assert_eq!(default_role(&NodeKind::Row), Role::GenericContainer);
}

#[test]
fn default_role_for_leaf_widgets() {
    assert_eq!(
        default_role(&NodeKind::Button {
            disabled: false,
            button_type: relay::vdom::ButtonType::Submit,
        }),
        Role::Button
    );
    assert_eq!(
        default_role(&NodeKind::Text {
            content: "x".into()
        }),
        Role::Label
    );
}
