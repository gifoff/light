use std::collections::HashMap;

use super::base::*;
use crate::vdom::{NodeKind, Resize, VElement, WhiteSpace};

impl super::base::Layout {
    /// Раскладывает дерево `root` в область `viewport` размерами `metrics`.
    ///
    /// Корневой узел всегда занимает весь viewport; его содержимое отступает
    /// на собственный `padding`/`border-size` (по умолчанию `0`).
    pub fn build(root: &VElement, viewport: (f32, f32), metrics: &dyn TextMetrics) -> Self {
        let mut layout = Self { nodes: Vec::new() };
        let view = Rect::new(0.0, 0.0, viewport.0, viewport.1);
        let metrics = MeasuredText::new(metrics);
        layout.place(
            root,
            Point::new(0.0, 0.0),
            FlowContext {
                content: ContentBox {
                    left: 0.0,
                    top: 0.0,
                    width: viewport.0,
                    height: viewport.1,
                },
                width_known: true,
                height_known: true,
                stretch: Some(Axis::Width),
                forced_w: None,
                forced_h: None,
            },
            None,
            &metrics,
            Inherited {
                size: FONT_SIZE,
                color: None,
                white_space: WhiteSpace::Normal,
            },
        );
        // Корень занимает всё окно независимо от содержимого.
        if let Some(node) = layout.nodes.first_mut() {
            node.rect = view;
        }
        // Соседи переупорядочиваются по `z` (порядок отрисовки и хит-теста).
        layout.reorder_by_z();
        layout
    }

    /// Плоский список узлов (в порядке отрисовки).
    pub fn nodes(&self) -> &[PlacedNode] {
        &self.nodes
    }

    /// Прямоугольник узла по ключу.
    pub fn rect_of(&self, key: &str) -> Option<Rect> {
        self.nodes
            .iter()
            .find(|node| node.key == key)
            .map(|node| node.rect)
    }

    /// Узел по ключу.
    pub fn node(&self, key: &str) -> Option<&PlacedNode> {
        self.nodes.iter().find(|node| node.key == key)
    }

    /// Применяет сохранённые смещения прокрутки, сдвигает прокручиваемое
    /// содержимое и вычисляет `clip_rect` для потомков.
    ///
    /// Вызывается после [`Self::build`] каждый раз, когда окно перерисовывается.
    /// Смещения клампятся по фактическому размеру содержимого.
    pub fn apply_scroll_offsets(&mut self, offsets: &HashMap<&'static str, Point>) {
        if self.nodes.is_empty() {
            return;
        }
        self.nodes[0].scroll_x = 0.0;
        self.nodes[0].scroll_y = 0.0;
        for index in 1..self.nodes.len() {
            let offset = offsets
                .get(&self.nodes[index].key)
                .copied()
                .unwrap_or(Point { x: 0.0, y: 0.0 });
            let node = &mut self.nodes[index];
            node.scroll_x = if node.overflow_x.scrollable() {
                offset.x.clamp(0.0, node.max_scroll_x())
            } else {
                0.0
            };
            node.scroll_y = if node.overflow_y.scrollable() {
                offset.y.clamp(0.0, node.max_scroll_y())
            } else {
                0.0
            };
        }
        self.nodes[0].clip_rect = None;
        let end = self.nodes.len();
        self.apply_clip_and_shift(0, end);
    }

    /// Самый глубокий прокручиваемый узел под точкой по выбранной оси.
    pub fn scroll_target(&self, point: Point, horizontal: bool) -> Option<&PlacedNode> {
        self.hit_stack(point).into_iter().find(|node| {
            if horizontal {
                node.overflow_x.scrollable() && node.max_scroll_x() > 0.5
            } else {
                node.overflow_y.scrollable() && node.max_scroll_y() > 0.5
            }
        })
    }

    /// Смещение прокрутки узла по ключу.
    pub fn scroll_offset(&self, key: &str) -> Point {
        self.node(key)
            .map(|node| Point::new(node.scroll_x, node.scroll_y))
            .unwrap_or(Point { x: 0.0, y: 0.0 })
    }

    pub(crate) fn apply_clip_and_shift(&mut self, index: usize, end: usize) {
        let node = &self.nodes[index];
        let scroll_x = node.scroll_x;
        let scroll_y = node.scroll_y;
        let own_clip = if node.overflow_x.clips() || node.overflow_y.clips() {
            let clip = node.content_rect();
            Some(match node.clip_rect {
                Some(parent) => parent.intersection(clip),
                None => clip,
            })
        } else {
            node.clip_rect
        };
        let children = node.children.clone();
        for (position, child) in children.iter().enumerate() {
            let child = *child;
            let child_end = children.get(position + 1).copied().unwrap_or(end);
            if scroll_x != 0.0 || scroll_y != 0.0 {
                for node in &mut self.nodes[child..child_end] {
                    node.rect.x -= scroll_x;
                    node.rect.y -= scroll_y;
                }
            }
            self.nodes[child].clip_rect = own_clip;
            self.apply_clip_and_shift(child, child_end);
        }
    }

    /// Интерактивный узел под точкой: кнопка или незаблокированное поле.
    ///
    /// Сначала проверяются поздние соседи и потомки (они выше в порядке
    /// отрисовки), поэтому при наложении возвращается верхний узел.
    pub fn hit_interactive(&self, point: Point) -> Option<&PlacedNode> {
        if self.nodes.is_empty() {
            return None;
        }
        self.hit(0, point)
    }

    /// Путь узлов под точкой от самого глубокого к корню.
    ///
    /// Используется оконным бэкендом для доставки события: начиная с самого
    /// глубокого узла, диспетчер поднимается вверх, пока обработчик не
    /// вернёт `true`. Гарантированно содержит хотя бы корневой узел, если
    /// точка внутри окна.
    pub fn hit_stack(&self, point: Point) -> Vec<&PlacedNode> {
        let mut stack = Vec::new();
        if !self.nodes.is_empty() {
            self.collect_hit(0, point, &mut stack);
        }
        stack
    }

    /// Рекурсивно собирает путь попаданий (потомки раньше предка).
    fn collect_hit<'a>(
        &'a self,
        index: usize,
        point: Point,
        stack: &mut Vec<&'a PlacedNode>,
    ) -> bool {
        let node = &self.nodes[index];
        // `pointer-events="none"` — узел не «под курсором»: он и все
        // потомки не попадают в hit-стек, поэтому не перехватывают мышь
        // (стандартное CSS-поведение: тултип не вытесняет мышь с родителя).
        if node.style.pointer_events == Some(false) {
            return false;
        }
        if !node.rect.contains(point) || node.clip_rect.is_some_and(|clip| !clip.contains(point)) {
            return false;
        }
        for &child in node.children.iter().rev() {
            if self.collect_hit(child, point, stack) {
                break;
            }
        }
        stack.push(node);
        true
    }

    /// Лежит ли точка внутри титульной полосы (`<titlebar>`).
    ///
    /// Используется оконным бэкендом, чтобы начать перетаскивание окна за
    /// заголовок.
    pub fn hit_titlebar(&self, point: Point) -> bool {
        self.nodes
            .iter()
            .any(|node| matches!(node.kind, NodeKind::TitleBar) && node.rect.contains(point))
    }

    /// Текстовое поле и направление изменения размера под точкой.
    ///
    /// Возвращает верхний узел, если точка попала в «захват» изменения размера
    /// (правый край для `horizontal`, нижний для `vertical`, угол для `both`).
    pub fn hit_resize(&self, point: Point) -> Option<(&PlacedNode, Resize)> {
        self.nodes.iter().rev().find_map(|node| {
            let NodeKind::TextField {
                resize,
                disabled: false,
                ..
            } = &node.kind
            else {
                return None;
            };
            if *resize == Resize::None || !node.rect.contains(point) {
                return None;
            }
            let right = node.rect.x + node.rect.w;
            let bottom = node.rect.y + node.rect.h;
            let near_right = point.x >= right - RESIZE_GRIP;
            let near_bottom = point.y >= bottom - RESIZE_GRIP;
            let hit = match resize {
                Resize::Horizontal => near_right,
                Resize::Vertical => near_bottom,
                Resize::Both => near_right && near_bottom,
                Resize::None => false,
            };
            hit.then_some((node, *resize))
        })
    }
}
