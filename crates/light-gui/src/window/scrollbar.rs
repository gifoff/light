//! Обработка скроллбаров: нажатие, перетаскивание, геометрия и повреждение
//! области скроллбара. Второй блок `impl WindowState` и свободные функции.

use std::collections::HashMap;

use light_vdom::layout::{Layout, PlacedNode, Point, Rect};

#[allow(unused_imports)]
use super::paint;
#[allow(unused_imports)]
use super::paint::Canvas;
#[allow(unused_imports)]
use super::state::ScaledMetrics;
use super::state::{ScrollDrag, ScrollbarHit, WindowState};

impl WindowState {
    /// Обрабатывает нажатие кнопки мыши на скроллбаре. Возвращает `true`,
    /// если нажатие было перехвачено (и обычные клики не нужны).
    pub(super) fn handle_scrollbar_press(&mut self) -> bool {
        let Some(hit) = self.scrollbar_at(self.cursor) else {
            return false;
        };
        if hit.on_thumb {
            let Some((_track, thumb)) = self
                .layout
                .as_ref()
                .and_then(|layout| layout.node(hit.key))
                .and_then(|node| scrollbar_geometry(node, self.scale, hit.horizontal))
            else {
                return false;
            };
            let grab = if hit.horizontal {
                self.cursor.x - thumb.x
            } else {
                self.cursor.y - thumb.y
            };
            let grab = if hit.horizontal {
                grab.clamp(0.0, thumb.w)
            } else {
                grab.clamp(0.0, thumb.h)
            };
            self.scroll_drag = Some(ScrollDrag {
                key: hit.key,
                horizontal: hit.horizontal,
                grab,
            });
            return true;
        }
        self.scroll_page(hit);
        true
    }

    /// Прокрутка на страницу (высоту окна контейнера) при клике по дорожке.
    fn scroll_page(&mut self, hit: ScrollbarHit) {
        let Some(node) = self
            .layout
            .as_ref()
            .and_then(|layout| layout.node(hit.key))
            .and_then(|node| {
                let max = if hit.horizontal {
                    node.max_scroll_x()
                } else {
                    node.max_scroll_y()
                };
                (max > 0.0).then_some((node, max))
            })
        else {
            return;
        };
        let (node, max) = node;
        let Some((_track, thumb)) = scrollbar_geometry(node, self.scale, hit.horizontal) else {
            return;
        };
        let offset = self.scroll.entry(hit.key).or_insert(Point::new(0.0, 0.0));
        let changed = if hit.horizontal {
            let forward = self.cursor.x > thumb.x + thumb.w;
            let next = if forward {
                offset.x + node.rect.w
            } else {
                offset.x - node.rect.w
            };
            let next = next.clamp(0.0, max);
            let changed = next != offset.x;
            offset.x = next;
            changed
        } else {
            let forward = self.cursor.y > thumb.y + thumb.h;
            let next = if forward {
                offset.y + node.rect.h
            } else {
                offset.y - node.rect.h
            };
            let next = next.clamp(0.0, max);
            let changed = next != offset.y;
            offset.y = next;
            changed
        };
        if changed {
            self.invalidate();
        }
    }

    /// Смещает прокрутку во время перетаскивания бегунка вслед за курсором.
    pub(super) fn apply_scroll_drag(&mut self) {
        let Some(drag) = self.scroll_drag else {
            return;
        };
        let Some(node) = self
            .layout
            .as_ref()
            .and_then(|layout| layout.node(drag.key))
        else {
            return;
        };
        let Some((track, thumb)) = scrollbar_geometry(node, self.scale, drag.horizontal) else {
            return;
        };
        let max = if drag.horizontal {
            node.max_scroll_x()
        } else {
            node.max_scroll_y()
        };
        let travel = if drag.horizontal {
            track.w - thumb.w
        } else {
            track.h - thumb.h
        };
        if travel <= 0.0 || max <= 0.0 {
            return;
        }
        let cursor_pos = if drag.horizontal {
            self.cursor.x
        } else {
            self.cursor.y
        };
        let track_start = if drag.horizontal { track.x } else { track.y };
        let target = (cursor_pos - drag.grab).clamp(track_start, track_start + travel);
        let next = max * (target - track_start) / travel;
        let offset = self.scroll.entry(drag.key).or_insert(Point::new(0.0, 0.0));
        let changed = if drag.horizontal {
            let changed = (offset.x - next).abs() > f32::EPSILON;
            offset.x = next;
            changed
        } else {
            let changed = (offset.y - next).abs() > f32::EPSILON;
            offset.y = next;
            changed
        };
        if changed {
            self.invalidate();
        }
    }

    /// Скроллбар под точкой: верхний (самый вложенный) контейнер, чья дорожка
    /// содержит точку.
    fn scrollbar_at(&self, point: Point) -> Option<ScrollbarHit> {
        let layout = self.layout.as_ref()?;
        layout.hit_stack(point).into_iter().find_map(|node| {
            for horizontal in [false, true] {
                if let Some((track, thumb)) = scrollbar_geometry(node, self.scale, horizontal)
                    && track.contains(point)
                {
                    return Some(ScrollbarHit {
                        key: node.key,
                        horizontal,
                        on_thumb: thumb.contains(point),
                    });
                }
            }
            None
        })
    }
}

pub(super) fn prune_scroll_offsets(scroll: &mut HashMap<&'static str, Point>, layout: &Layout) {
    scroll.retain(|key, _| {
        layout
            .node(key)
            .is_some_and(|node| node.max_scroll_x() > 0.5 || node.max_scroll_y() > 0.5)
    });
}

/// Полосы скроллбаров всех прокручиваемых контейнеров в старой и новой
/// раскладке.
///
/// Используется для перерисовки области, где раньше была полоса прокрутки:
/// после того как контент стал короче и полоса исчезла, её пиксели иначе
/// остались бы на экране («фантомный» скроллбар).
pub(super) fn scrollbar_strip_damage(old: &Layout, new: &Layout, scale: f32) -> Vec<Rect> {
    let bar = paint::SCROLLBAR_SIZE * scale;
    let mut rects = Vec::new();
    let mut collect = |node: &PlacedNode| {
        if !(node.overflow_x.scrollable() || node.overflow_y.scrollable()) {
            return;
        }
        let rect = node.rect;
        if node.overflow_y.scrollable() {
            rects.push(Rect::new(rect.x + rect.w - bar, rect.y, bar, rect.h));
        }
        if node.overflow_x.scrollable() {
            let width = if node.overflow_y.scrollable() {
                (rect.w - bar).max(0.0)
            } else {
                rect.w
            };
            rects.push(Rect::new(rect.x, rect.y + rect.h - bar, width, bar));
        }
    };
    for node in old.nodes() {
        collect(node);
    }
    for node in new.nodes() {
        collect(node);
    }
    rects
}

/// Возвращает геометрию скроллбара узла по выбранной оси: дорожку и бегунок.
/// Возвращает `None`, если по этой оси нет прокрутки или узел не прокручивается.
pub(super) fn scrollbar_geometry(
    node: &PlacedNode,
    scale: f32,
    horizontal: bool,
) -> Option<(Rect, Rect)> {
    if horizontal {
        if !(node.overflow_x.scrollable() && node.max_scroll_x() > 0.5) {
            return None;
        }
    } else if !(node.overflow_y.scrollable() && node.max_scroll_y() > 0.5) {
        return None;
    }
    let rect = node.rect;
    let bar = paint::SCROLLBAR_SIZE * scale;
    let min_thumb = (paint::SCROLLBAR_MIN_THUMB * scale).min(bar * 2.0);
    if horizontal {
        let width = if node.max_scroll_y() > 0.5 {
            (rect.w - bar).max(0.0)
        } else {
            rect.w
        };
        let track = Rect::new(rect.x, rect.y + rect.h - bar, width, bar);
        let thumb_w =
            (track.w.max(0.0) * rect.w / node.scroll_width).clamp(min_thumb.min(track.w), track.w);
        let travel = (track.w - thumb_w).max(0.0);
        let position = if node.max_scroll_x() > 0.0 {
            node.scroll_x / node.max_scroll_x()
        } else {
            0.0
        };
        let thumb = Rect::new(track.x + travel * position, track.y, thumb_w, bar);
        Some((track, thumb))
    } else {
        let track = Rect::new(rect.x + rect.w - bar, rect.y, bar, rect.h);
        let thumb_h =
            (track.h.max(0.0) * rect.h / node.scroll_height).clamp(min_thumb.min(track.h), track.h);
        let travel = (track.h - thumb_h).max(0.0);
        let position = if node.max_scroll_y() > 0.0 {
            node.scroll_y / node.max_scroll_y()
        } else {
            0.0
        };
        let thumb = Rect::new(track.x, track.y + travel * position, bar, thumb_h);
        Some((track, thumb))
    }
}
