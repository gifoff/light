//! Тесты встроенных клавиатурных сокращений.
#![cfg(feature = "a11y-keyboard")]

extern crate light_fw_core as light;

use light::a11y::keyboard::{builtin_action, is_activation_key, is_builtin};

#[test]
fn tab_without_shift_is_builtin() {
    assert!(is_builtin("Tab", false));
    assert!(builtin_action("Tab", false).is_some());
}

#[test]
fn tab_with_shift_is_builtin() {
    assert!(is_builtin("Tab", true));
}

#[test]
fn escape_is_builtin() {
    assert!(is_builtin("Escape", false));
}

#[test]
fn arrow_keys_are_builtin() {
    for key in ["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"] {
        assert!(is_builtin(key, false), "key = {key}");
    }
}

#[test]
fn activation_keys_detected() {
    assert!(is_activation_key("Enter"));
    assert!(is_activation_key(" "));
    assert!(is_activation_key("Space"));
    assert!(!is_activation_key("Tab"));
    assert!(!is_activation_key("Escape"));
}

#[test]
fn unknown_key_is_not_builtin() {
    assert!(!is_builtin("F5", false));
    assert!(!is_builtin("q", false));
    assert!(builtin_action("F5", false).is_none());
}
