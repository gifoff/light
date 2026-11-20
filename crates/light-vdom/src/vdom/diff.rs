use std::collections::{HashMap, HashSet};

use super::node::{NodeKind, VElement};
use super::ops::{Op, Patch};

/// Сравнивает два дерева и возвращает список точечных изменений.
pub fn diff(old: &VElement, new: &VElement) -> Patch {
    let mut patch = Patch::default();
    diff_node(old, new, &mut patch);
    patch
}

/// Внутренняя рекурсивная функция диффа узла.
pub fn diff_node(old: &VElement, new: &VElement, patch: &mut Patch) {
    if old.key != new.key {
        debug_assert!(false, "diff_node вызван для разных ключей");
        patch.ops.push(Op::Replace { key: new.key });
        patch.changed.insert(new.key);
        return;
    }
    if !kind_compatible(&old.kind, &new.kind) {
        patch.ops.push(Op::Replace { key: new.key });
        patch.changed.insert(new.key);
        return;
    }
    if old.style != new.style {
        patch.changed.insert(new.key);
    }
    if old.attrs != new.attrs {
        patch.changed.insert(new.key);
    }
    diff_kind(&old.kind, &new.kind, new.key, patch);
    diff_children(old, new, patch);
}

/// Совместимы ли типы узлов: контейнер с контейнером, текст с текстом и т. д.
/// Проверяет совместимость двух типов узлов для точечного патчинга.
pub fn kind_compatible(a: &NodeKind, b: &NodeKind) -> bool {
    match (a, b) {
        (NodeKind::Column, NodeKind::Column)
        | (NodeKind::Row, NodeKind::Row)
        | (NodeKind::TitleBar, NodeKind::TitleBar)
        | (NodeKind::Window, NodeKind::Window)
        | (NodeKind::Text { .. }, NodeKind::Text { .. })
        | (NodeKind::Button { .. }, NodeKind::Button { .. })
        | (NodeKind::TextField { .. }, NodeKind::TextField { .. }) => true,
        #[cfg(feature = "image-support")]
        (NodeKind::Image { src: src_a, .. }, NodeKind::Image { src: src_b, .. }) => src_a == src_b,
        #[cfg(feature = "image-support")]
        (NodeKind::RuntimeImage { image: a, .. }, NodeKind::RuntimeImage { image: b, .. }) => {
            a.key == b.key
        }
        #[cfg(feature = "elements-checkbox")]
        (NodeKind::Checkbox { .. }, NodeKind::Checkbox { .. }) => true,
        #[cfg(feature = "elements-radio")]
        (NodeKind::Radio { .. }, NodeKind::Radio { .. }) => true,
        #[cfg(feature = "elements-select")]
        (NodeKind::Select { .. }, NodeKind::Select { .. }) => true,
        // Кастомные элементы совместимы, только если их теги совпадают:
        // смена тега — это смена плагина/компонента, а не обновление в place.
        (NodeKind::Custom(a), NodeKind::Custom(b)) => a == b,
        _ => false,
    }
}

/// Сравнивает содержимое двух узлов одного типа и генерирует оп.
pub fn diff_kind(old: &NodeKind, new: &NodeKind, key: &'static str, patch: &mut Patch) {
    match (old, new) {
        (NodeKind::Text { content: a }, NodeKind::Text { content: b }) => {
            if a != b {
                patch.ops.push(Op::SetContent {
                    key,
                    content: b.clone(),
                });
                patch.changed.insert(key);
            }
        }
        #[cfg(feature = "image-support")]
        (
            NodeKind::Image {
                data: data_a,
                alt: alt_a,
                ..
            },
            NodeKind::Image {
                data: data_b,
                alt: alt_b,
                ..
            },
        ) => {
            if data_a != data_b || alt_a != alt_b {
                patch.changed.insert(key);
            }
        }
        #[cfg(feature = "image-support")]
        (
            NodeKind::RuntimeImage {
                image: image_a,
                alt: alt_a,
            },
            NodeKind::RuntimeImage {
                image: image_b,
                alt: alt_b,
            },
        ) => {
            if image_a != image_b || alt_a != alt_b {
                patch.changed.insert(key);
            }
        }
        (
            NodeKind::Button {
                disabled: disabled_a,
                button_type: type_a,
            },
            NodeKind::Button {
                disabled: disabled_b,
                button_type: button_type_b,
            },
        ) => {
            if disabled_a != disabled_b {
                patch.ops.push(Op::SetDisabled {
                    key,
                    disabled: *disabled_b,
                });
                patch.changed.insert(key);
            }
            if type_a != button_type_b {
                patch.changed.insert(key);
            }
        }
        (
            NodeKind::TextField {
                value: value_a,
                placeholder: placeholder_a,
                disabled: disabled_a,
                multiline: multiline_a,
                resize: resize_a,
                masked: masked_a,
            },
            NodeKind::TextField {
                value: value_b,
                placeholder: placeholder_b,
                disabled: disabled_b,
                multiline: multiline_b,
                resize: resize_b,
                masked: masked_b,
            },
        ) => {
            if value_a != value_b {
                patch.ops.push(Op::SetValue {
                    key,
                    value: value_b.clone(),
                });
                patch.changed.insert(key);
            }
            if placeholder_a != placeholder_b {
                patch.ops.push(Op::SetPlaceholder {
                    key,
                    placeholder: placeholder_b.clone(),
                });
                patch.changed.insert(key);
            }
            if disabled_a != disabled_b {
                patch.ops.push(Op::SetDisabled {
                    key,
                    disabled: *disabled_b,
                });
                patch.changed.insert(key);
            }
            if multiline_a != multiline_b {
                patch.changed.insert(key);
            }
            if resize_a != resize_b {
                patch.changed.insert(key);
            }
            if masked_a != masked_b {
                patch.ops.push(Op::SetMasked {
                    key,
                    masked: *masked_b,
                });
                patch.changed.insert(key);
            }
        }
        #[cfg(feature = "elements-checkbox")]
        (
            NodeKind::Checkbox {
                label: label_a,
                checked: checked_a,
                disabled: disabled_a,
            },
            NodeKind::Checkbox {
                label: label_b,
                checked: checked_b,
                disabled: disabled_b,
            },
        ) => {
            if label_a != label_b {
                patch.ops.push(Op::SetLabel {
                    key,
                    label: label_b.clone(),
                });
                patch.changed.insert(key);
            }
            if checked_a != checked_b {
                patch.ops.push(Op::SetChecked {
                    key,
                    checked: *checked_b,
                });
                patch.changed.insert(key);
            }
            if disabled_a != disabled_b {
                patch.ops.push(Op::SetDisabled {
                    key,
                    disabled: *disabled_b,
                });
                patch.changed.insert(key);
            }
        }
        #[cfg(feature = "elements-radio")]
        (
            NodeKind::Radio {
                label: label_a,
                value: value_a,
                selected: selected_a,
                disabled: disabled_a,
            },
            NodeKind::Radio {
                label: label_b,
                value: value_b,
                selected: selected_b,
                disabled: disabled_b,
            },
        ) => {
            if label_a != label_b {
                patch.ops.push(Op::SetLabel {
                    key,
                    label: label_b.clone(),
                });
                patch.changed.insert(key);
            }
            if value_a != value_b {
                patch.changed.insert(key);
            }
            if selected_a != selected_b {
                patch.ops.push(Op::SetSelected {
                    key,
                    selected: *selected_b,
                });
                patch.changed.insert(key);
            }
            if disabled_a != disabled_b {
                patch.ops.push(Op::SetDisabled {
                    key,
                    disabled: *disabled_b,
                });
                patch.changed.insert(key);
            }
        }
        #[cfg(feature = "elements-select")]
        (
            NodeKind::Select {
                value: value_a,
                options: options_a,
                disabled: disabled_a,
            },
            NodeKind::Select {
                value: value_b,
                options: options_b,
                disabled: disabled_b,
            },
        ) => {
            if value_a != value_b {
                patch.ops.push(Op::SetValue {
                    key,
                    value: value_b.clone(),
                });
                patch.changed.insert(key);
            }
            if options_a != options_b {
                patch.ops.push(Op::SetOptions {
                    key,
                    options: options_b.clone(),
                });
                patch.changed.insert(key);
            }
            if disabled_a != disabled_b {
                patch.ops.push(Op::SetDisabled {
                    key,
                    disabled: *disabled_b,
                });
                patch.changed.insert(key);
            }
        }
        (NodeKind::Column, NodeKind::Column)
        | (NodeKind::Row, NodeKind::Row)
        | (NodeKind::TitleBar, NodeKind::TitleBar)
        | (NodeKind::Window, NodeKind::Window) => {}
        // Кастомный элемент — атомарное целое: его внутреннее состояние
        // живёт в плагин-экземпляре (custom_instances), а не в VDOM, поэтому
        // diff ничего не генерирует. Равенство тегов уже проверено в
        // kind_compatible.
        (NodeKind::Custom(_), NodeKind::Custom(_)) => {}
        _ => debug_assert!(false, "несовместимые типы должны фильтроваться раньше"),
    }
}

/// Сравнивает списки детей двух узлов и генерирует точечные оп для изменений.
pub fn diff_children(old: &VElement, new: &VElement, patch: &mut Patch) {
    let old_by_key: HashMap<&'static str, &VElement> = old
        .children
        .iter()
        .map(|child| (child.key, child))
        .collect();
    let mut matched = HashSet::new();
    for new_child in &new.children {
        match old_by_key.get(new_child.key) {
            Some(old_child) => {
                matched.insert(new_child.key);
                diff_node(old_child, new_child, patch);
            }
            None => {
                patch.ops.push(Op::Insert { key: new_child.key });
                patch.changed.insert(new_child.key);
            }
        }
    }
    for old_child in &old.children {
        if !matched.contains(old_child.key) {
            patch.ops.push(Op::Remove { key: old_child.key });
            patch.changed.insert(old_child.key);
        }
    }
}
