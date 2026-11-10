//! Integration tests for `post_update_to` window-token addressing.
//!
//! ТЗ 0.12-B-fix5: verify that a `post_update_to` carrying the first
//! window's token updates only that window and leaves others untouched.
//! Since the winit event loop cannot be driven from an integration test,
//! the ТЗ-sanctioned fallback extracts the dispatch logic into
//! [`light_fw::apply_action`] and exercises it directly with stub components.

#![cfg(feature = "async-runtime")]

extern crate light_fw_core as light_fw;

use std::any::TypeId;

use light_fw::{Component, VElement, WindowToken};

#[derive(Debug)]
struct First {
    count: u32,
}

impl Component for First {
    fn render(&self) -> VElement {
        VElement::leaf(
            "first",
            light_fw::NodeKind::Text {
                content: format!("first = {}", self.count),
            },
        )
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

#[derive(Debug)]
struct Second {
    count: u32,
}

impl Component for Second {
    fn render(&self) -> VElement {
        VElement::leaf(
            "second",
            light_fw::NodeKind::Text {
                content: format!("second = {}", self.count),
            },
        )
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

fn two_windows<'a>(
    first: &'a mut First,
    second: &'a mut Second,
) -> impl Iterator<Item = (WindowToken, TypeId, &'a mut dyn Component)> {
    vec![
        (1, TypeId::of::<First>(), first as &mut dyn Component),
        (2, TypeId::of::<Second>(), second as &mut dyn Component),
    ]
    .into_iter()
}

/// Exact token addressing: token 1 → only `First` is mutated.
#[test]
fn post_update_to_first_updates_only_first() {
    let mut first = First { count: 0 };
    let mut second = Second { count: 0 };

    let applied = light_fw::apply_action(
        two_windows(&mut first, &mut second),
        Some(1),
        TypeId::of::<First>(),
        Box::new(|any: &mut dyn std::any::Any| {
            if let Some(c) = any.downcast_mut::<First>() {
                c.count += 1;
            }
        }),
    );

    assert_eq!(applied, Some(1));
    assert_eq!(first.count, 1);
    assert_eq!(second.count, 0);
}

/// Exact token addressing: token 2 → only `Second` is mutated.
#[test]
fn post_update_to_second_updates_only_second() {
    let mut first = First { count: 0 };
    let mut second = Second { count: 0 };

    let applied = light_fw::apply_action(
        two_windows(&mut first, &mut second),
        Some(2),
        TypeId::of::<Second>(),
        Box::new(|any: &mut dyn std::any::Any| {
            if let Some(c) = any.downcast_mut::<Second>() {
                c.count += 1;
            }
        }),
    );

    assert_eq!(applied, Some(2));
    assert_eq!(first.count, 0);
    assert_eq!(second.count, 1);
}

/// Unknown token: the window is "closed" — the action is discarded.
#[test]
fn unknown_token_returns_none_and_no_mutation() {
    let mut first = First { count: 0 };
    let mut second = Second { count: 0 };

    let applied = light_fw::apply_action(
        two_windows(&mut first, &mut second),
        Some(99),
        TypeId::of::<First>(),
        Box::new(|any: &mut dyn std::any::Any| {
            if let Some(c) = any.downcast_mut::<First>() {
                c.count += 1;
            }
        }),
    );

    assert_eq!(applied, None);
    assert_eq!(first.count, 0);
    assert_eq!(second.count, 0);
}

/// Broadcast (`token = None`): action applies to the first window whose
/// component matches `target` by `TypeId` — here `First` (token 1).
#[test]
fn broadcast_updates_first_matching_typeid() {
    let mut first = First { count: 0 };
    let mut second = Second { count: 0 };

    let applied = light_fw::apply_action(
        two_windows(&mut first, &mut second),
        None,
        TypeId::of::<First>(),
        Box::new(|any: &mut dyn std::any::Any| {
            if let Some(c) = any.downcast_mut::<First>() {
                c.count += 1;
            }
        }),
    );

    assert_eq!(applied, Some(1));
    assert_eq!(first.count, 1);
    assert_eq!(second.count, 0);
}

/// Broadcast with a `TypeId` that matches no window: action is discarded.
#[test]
fn broadcast_with_no_match_returns_none() {
    let mut first = First { count: 0 };
    let mut second = Second { count: 0 };

    let applied = light_fw::apply_action(
        two_windows(&mut first, &mut second),
        None,
        TypeId::of::<u32>(),
        Box::new(|_any: &mut dyn std::any::Any| {}),
    );

    assert_eq!(applied, None);
    assert_eq!(first.count, 0);
    assert_eq!(second.count, 0);
}
