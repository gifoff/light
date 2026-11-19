use super::base::*;
use super::regions::aligned;
use crate::vdom::{Align, Position, Size, VElement};

impl super::base::Layout {
    /// Повторно раскладывает детей горизонтального контейнера, растягивая
    /// детей с `height:auto` на итоговую высоту content box.
    pub(crate) fn stretch_row_children(
        &mut self,
        vnode: &VElement,
        index: usize,
        content: ContentBox,
        metrics: &dyn TextMetrics,
        inherited: Inherited,
    ) -> (f32, f32) {
        let Inherited {
            size: font_size,
            color: font_color,
            white_space,
        } = inherited;
        let needed = self.nodes[index].children.iter().any(|&child| {
            let node = &self.nodes[child];
            let explicit_position = vnode
                .children
                .iter()
                .find(|entry| entry.key == node.key)
                .is_some_and(|entry| entry.attr("position-y").is_some());
            let stretch = node.style.height == Size::Auto
                && node.style.align_y == Align::Start
                && !explicit_position;
            stretch && (content.height - node.style.margin.vertical() - node.rect.h).abs() > 0.5
        });
        if !needed {
            return self.row_content_size(index);
        }
        let saved: Vec<(&'static str, f32)> = self.nodes[index]
            .children
            .iter()
            .map(|&child| (self.nodes[child].key, self.nodes[child].rect.w))
            .collect();
        self.nodes.truncate(index + 1);
        self.nodes[index].children.clear();
        let mut cursor_x = content.left;
        for child in &vnode.children {
            if !child.style.visible || child.style.position == Position::Absolute {
                continue;
            }
            let Some(&(_, width)) = saved.iter().find(|(key, _)| *key == child.key) else {
                continue;
            };
            let explicit_position = child.attr("position-y").is_some();
            let height = if child.style.height == Size::Auto
                && child.style.align_y == Align::Start
                && !explicit_position
            {
                Some((content.height - child.style.margin.vertical()).max(0.0))
            } else {
                None
            };
            let (child_w, _) = self.place(
                child,
                Point::new(cursor_x, content.top),
                FlowContext {
                    content,
                    width_known: true,
                    height_known: true,
                    stretch: None,
                    forced_w: Some(width),
                    forced_h: height,
                },
                Some(index),
                metrics,
                Inherited {
                    size: font_size,
                    color: font_color,
                    white_space,
                },
            );
            cursor_x += child_w + SPACING;
        }
        self.row_content_size(index)
    }

    pub(crate) fn row_content_size(&self, index: usize) -> (f32, f32) {
        let children = &self.nodes[index].children;
        let mut width = 0.0_f32;
        let mut height = 0.0_f32;
        for &child in children {
            let node = &self.nodes[child];
            width += node.rect.w + node.style.margin.horizontal();
            height = height.max(node.rect.h + node.style.margin.vertical());
        }
        if children.len() > 1 {
            width += SPACING * (children.len() - 1) as f32;
        }
        (width, height)
    }

    /// Выравнивает прямых детей узла `index` по `justify-content`/`align-items`
    /// относительно его контентной области `content` и доступного размера `used_main`.
    ///
    /// `explicit_cross` — для каждого прямого ребёнка указывает, задан ли у него
    /// собственный `position-x`/`position-y` по cross-оси: такие дети не сдвигаются
    /// здесь, чтобы их `position` переопределял `align-items` родителя.
    pub(crate) fn align_container_content(
        &mut self,
        index: usize,
        content: ContentBox,
        horizontal: bool,
        used_main: f32,
        explicit_cross: &[bool],
    ) {
        let style = self.nodes[index].style;
        let justify = style.justify_content;
        let align_items = style.align_items;
        if justify == Align::Start && align_items == Align::Start {
            return;
        }
        let (main_avail, cross_avail) = if horizontal {
            (content.width, content.height)
        } else {
            (content.height, content.width)
        };
        let free_main = main_avail - used_main;
        let delta_main = match justify {
            Align::Start => 0.0,
            Align::Center => free_main / 2.0,
            Align::End => free_main,
        };
        let children = self.nodes[index].children.clone();
        let end = self.nodes.len();
        let mut flow_position = 0;
        for (position, &child) in children.iter().enumerate() {
            if self.nodes[child].style.position == Position::Absolute {
                continue;
            }
            let child_end = children.get(position + 1).copied().unwrap_or(end);
            let rect = self.nodes[child].rect;
            let explicit = explicit_cross.get(flow_position).copied().unwrap_or(false);
            flow_position += 1;
            let delta_cross = if explicit {
                0.0
            } else {
                let free_cross = if horizontal {
                    cross_avail - rect.h
                } else {
                    cross_avail - rect.w
                };
                match align_items {
                    Align::Start => 0.0,
                    Align::Center => free_cross / 2.0,
                    Align::End => free_cross,
                }
            };
            let (dx, dy) = if horizontal {
                (delta_main, delta_cross)
            } else {
                (delta_cross, delta_main)
            };
            if dx != 0.0 || dy != 0.0 {
                for node in &mut self.nodes[child..child_end] {
                    node.rect.x += dx;
                    node.rect.y += dy;
                }
            }
        }
    }

    /// Выравнивает прямых детей узла `index` по `position-x`/`position-y` и
    /// `position-offset-*` относительно его контентной области `content`.
    ///
    /// Абсолютные размеры и координаты фиксированы до вызова, поэтому всё
    /// поддерево ребёнка смещается одной дельтой — относительные позиции
    /// внутри него (в том числе у внуков) сохраняются.
    pub(crate) fn align_children(&mut self, index: usize, content: ContentBox) {
        let children = self.nodes[index].children.clone();
        let end = self.nodes.len();
        for (position, &child) in children.iter().enumerate() {
            let child_end = children.get(position + 1).copied().unwrap_or(end);
            let rect = self.nodes[child].rect;
            let style = self.nodes[child].style;
            let final_x = aligned(
                rect.x,
                rect.w,
                content.left,
                content.width,
                style.align_x,
                style.offset_x,
            );
            let final_y = aligned(
                rect.y,
                rect.h,
                content.top,
                content.height,
                style.align_y,
                style.offset_y,
            );
            let dx = final_x - rect.x;
            let dy = final_y - rect.y;
            if dx != 0.0 || dy != 0.0 {
                for node in &mut self.nodes[child..child_end] {
                    node.rect.x += dx;
                    node.rect.y += dy;
                }
            }
        }
    }

    /// Переупорядочивает плоский список так, чтобы среди соседей порядок
    /// отрисовки и попадания задавался `z`; геометрия узлов не меняется.
    ///
    /// Дети каждого узла стабильно сортируются по `style.z` (равные `z`
    /// сохраняют DOM-порядок), затем список перестраивается в обходе в глубину
    /// по новому порядку детей, а индексы детей перенумеровываются. `z`
    /// действует только среди соседей (поддерево остаётся внутри родителя).
    /// Корень остаётся нулевым индексом.
    pub(crate) fn reorder_by_z(&mut self) {
        if self.nodes.is_empty() {
            return;
        }
        let z: Vec<i32> = self.nodes.iter().map(|node| node.style.z).collect();
        for node in &mut self.nodes {
            node.children.sort_by_key(|&child| z[child]);
        }
        let mut order = Vec::with_capacity(self.nodes.len());
        let mut stack = vec![0usize];
        while let Some(index) = stack.pop() {
            order.push(index);
            for &child in self.nodes[index].children.iter().rev() {
                stack.push(child);
            }
        }
        if order.len() != self.nodes.len() {
            return;
        }
        let mut remap = vec![0usize; self.nodes.len()];
        for (new, &old) in order.iter().enumerate() {
            remap[old] = new;
        }
        let mut old_nodes: Vec<Option<PlacedNode>> = self.nodes.drain(..).map(Some).collect();
        let mut nodes = Vec::with_capacity(order.len());
        for &old in &order {
            let Some(mut node) = old_nodes[old].take() else {
                return;
            };
            for child in &mut node.children {
                *child = remap[*child];
            }
            nodes.push(node);
        }
        self.nodes = nodes;
    }

    pub(crate) fn push(&mut self, vnode: &VElement, rect: Rect, parent: Option<usize>) -> usize {
        self.nodes.push(PlacedNode {
            key: vnode.key,
            kind: vnode.kind.clone(),
            style: vnode.style,
            rect,
            children: Vec::new(),
            overflow_x: vnode.style.overflow_x,
            overflow_y: vnode.style.overflow_y,
            scroll_width: rect.w,
            scroll_height: rect.h,
            scroll_x: 0.0,
            scroll_y: 0.0,
            clip_rect: None,
        });
        let index = self.nodes.len() - 1;
        if let Some(parent) = parent {
            self.nodes[parent].children.push(index);
        }
        index
    }
}
