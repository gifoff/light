use std::collections::{BTreeSet, HashMap};

#[cfg(debug_assertions)]
use std::env;

use super::base::{FlowContext, Layout, Rect};
use crate::vdom::{Align, NodeKind, Offset, VElement};

/// Заблокирован ли узел директивой `:disabled`.
pub(crate) fn is_disabled(kind: &NodeKind) -> bool {
    match kind {
        NodeKind::Button { disabled, .. } | NodeKind::TextField { disabled, .. } => *disabled,
        #[cfg(feature = "elements-checkbox")]
        NodeKind::Checkbox { disabled, .. } => *disabled,
        #[cfg(feature = "elements-radio")]
        NodeKind::Radio { disabled, .. } => *disabled,
        #[cfg(feature = "elements-select")]
        NodeKind::Select { disabled, .. } => *disabled,
        _ => false,
    }
}

/// Вычисляет области перерисовки между двумя раскладками.
///
/// Возвращаются объединения старого и нового прямоугольников для узлов,
/// которые: были добавлены, удалены, сдвинуты/изменили размер, либо чьё
/// содержимое изменилось (по набору `changed` из [`crate::vdom::diff`]).
/// Если возвращён список пуст, перерисовывать окно не нужно.
pub fn damage_regions(
    old_rects: &HashMap<&'static str, Rect>,
    new: &Layout,
    changed: &BTreeSet<&'static str>,
) -> Vec<Rect> {
    let mut regions: HashMap<&'static str, Rect> = HashMap::new();
    for node in new.nodes() {
        let previous = old_rects.get(node.key).copied();
        let moved = previous != Some(node.rect);
        let content_changed = changed.contains(node.key);
        if moved || content_changed {
            let union = match previous {
                Some(old) => old.union(node.rect),
                None => node.rect,
            };
            regions
                .entry(node.key)
                .and_modify(|rect| *rect = rect.union(union))
                .or_insert(union);
        }
    }
    for (&key, &rect) in old_rects {
        if new.rect_of(key).is_none() {
            regions
                .entry(key)
                .and_modify(|r| *r = r.union(rect))
                .or_insert(rect);
        }
    }
    regions.into_values().collect()
}

/// Объединяет пересекающиеся области перерисовки в общие прямоугольники.
///
/// Перерисовка каждой области требует отдельного прохода по дереву, поэтому
/// наложение областей (например, родитель и изменившийся потомок) заметно
/// замедляет кадр. Объединение уменьшает число проходов, слегка расширяя
/// повреждённую площадь.
pub fn coalesce_regions(regions: Vec<Rect>) -> Vec<Rect> {
    let mut merged = Vec::<Rect>::with_capacity(regions.len());
    for rect in regions {
        if rect.w <= 0.0 || rect.h <= 0.0 {
            continue;
        }
        let mut current = rect;
        loop {
            let mut found = false;
            let mut index = 0;
            while index < merged.len() {
                if overlaps(merged[index], current) {
                    current = merged.swap_remove(index).union(current);
                    found = true;
                } else {
                    index += 1;
                }
            }
            if !found {
                break;
            }
        }
        merged.push(current);
    }
    merged
}

pub(crate) fn overlaps(a: Rect, b: Rect) -> bool {
    a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
}

/// Снимок прямоугольников всех узлов раскладки (для сравнения между кадрами).
pub fn snapshot_rects(layout: &Layout) -> HashMap<&'static str, Rect> {
    layout
        .nodes()
        .iter()
        .map(|node| (node.key, node.rect))
        .collect()
}

/// Итоговая координата узла по оси с учётом `position` и `position-offset`.
///
/// `flow` — обычная координата в потоке, `size` — размер узла, `bound_start`/
/// `bound_size` — контентная область родителя по оси.
/// Вес flex-ребёнка по main-оси. Titlebar — элемент оформления окна, поэтому
/// при нехватке места он не сжимается; иначе он может исчезнуть, а соседняя
/// scroll-область выйдет за пределы окна.
pub(crate) fn flex_weight(child: &VElement, growing: bool) -> f32 {
    if growing {
        child.style.flex_grow
    } else if matches!(child.kind, NodeKind::TitleBar) {
        0.0
    } else {
        child.style.flex_shrink
    }
}

pub(crate) fn aligned(
    flow: f32,
    size: f32,
    bound_start: f32,
    bound_size: f32,
    align: Align,
    offset: Offset,
) -> f32 {
    let base = match align {
        Align::Start => flow,
        Align::Center => bound_start + (bound_size - size) / 2.0,
        Align::End => bound_start + bound_size - size,
    };
    base + offset.resolve(bound_size)
}

#[cfg(debug_assertions)]
pub(crate) fn debug_trace(vnode: &VElement, rect: Rect, flow: FlowContext) {
    let dbg = env::var("LIGHT_DEBUG_LAYOUT").unwrap_or_default();
    if dbg != "1" && dbg != "true" && dbg != "on" {
        return;
    }
    eprintln!(
        "[LAYOUT] tag={} kind={:?} key={} parent_content=(x={} y={} w={} h={}) pos_x={:?} pos_y={:?} resolved_rect=(x={} y={} w={} h={})",
        vnode.tag,
        vnode.kind,
        vnode.key,
        flow.content.left,
        flow.content.top,
        flow.content.width,
        flow.content.height,
        vnode.style.align_x,
        vnode.style.align_y,
        rect.x,
        rect.y,
        rect.w,
        rect.h
    );
}
