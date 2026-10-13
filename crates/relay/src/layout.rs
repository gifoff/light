//! Раскладка виртуального дерева в прямоугольники, поиск узлов по точке
//! и вычисление областей перерисовки.

use std::collections::{BTreeSet, HashMap};

use crate::vdom::{Align, NodeKind, Offset, Resize, Style, VElement};

/// Шрифт интерфейса по умолчанию (px).
pub const FONT_SIZE: f32 = 16.0;
/// Расстояние между соседями в контейнере.
pub const SPACING: f32 = 6.0;
/// Горизонтальный отступ текста в кнопке.
pub const BUTTON_PAD_X: f32 = 14.0;
/// Вертикальный отступ текста в кнопке.
pub const BUTTON_PAD_Y: f32 = 9.0;
/// Минимальная высота кнопки.
pub const BUTTON_MIN_H: f32 = 34.0;
/// Ширина текстового поля.
pub const FIELD_WIDTH: f32 = 240.0;
/// Горизонтальный отступ текста в поле.
pub const FIELD_PAD_X: f32 = 8.0;
/// Минимальная высота однострочного текстового поля.
pub const FIELD_MIN_H: f32 = 34.0;
/// Высота одной строки многострочного поля.
pub const FIELD_LINE_H: f32 = 22.0;
/// Размер зоны захвата для изменения размера поля в углу.
pub const RESIZE_GRIP: f32 = 12.0;
/// Сторона квадрата флажка (`<checkbox>`).
pub const CHECKBOX_BOX: f32 = 18.0;
/// Зазор между флажком/переключателем и подписью.
pub const CHECKBOX_GAP: f32 = 8.0;
/// Ширина выпадающего списка (`<select>`).
pub const SELECT_WIDTH: f32 = 240.0;
/// Горизонтальный отступ текста в выпадающем списке.
pub const SELECT_PAD_X: f32 = 8.0;

/// Точка в пикселях (координаты окна).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    /// Горизонтальная координата.
    pub x: f32,
    /// Вертикальная координата.
    pub y: f32,
}

impl Point {
    /// Точка с заданными координатами.
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// Прямоугольник в пикселях (координаты окна).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Rect {
    /// Левая граница.
    pub x: f32,
    /// Верхняя граница.
    pub y: f32,
    /// Ширина.
    pub w: f32,
    /// Высота.
    pub h: f32,
}

impl Rect {
    /// Прямоугольник из координат левого верхнего угла и размеров.
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    /// Содержит ли прямоугольник точку (включая границы).
    pub fn contains(&self, point: Point) -> bool {
        point.x >= self.x
            && point.x <= self.x + self.w
            && point.y >= self.y
            && point.y <= self.y + self.h
    }

    /// Объединение двух прямоугольников.
    pub fn union(self, other: Self) -> Self {
        if self.w <= 0.0 || self.h <= 0.0 {
            return other;
        }
        if other.w <= 0.0 || other.h <= 0.0 {
            return self;
        }
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        let right = (self.x + self.w).max(other.x + other.w);
        let bottom = (self.y + self.h).max(other.y + other.h);
        Self::new(x, y, right - x, bottom - y)
    }
}

/// Контентная область элемента — единая точка отсчёта для детей.
///
/// Это прямоугольник внутри `padding` и рамки родителя: границы минус
/// `padding` и `border-size`. Относительно него разрешаются проценты
/// (`width`/`height`/`min-*`/`max-*`, `position-offset-*`) и выполняется
/// выравнивание `position-x`/`position-y` детей.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContentBox {
    /// Левая граница контентной области.
    pub left: f32,
    /// Верхняя граница.
    pub top: f32,
    /// Ширина.
    pub width: f32,
    /// Высота.
    pub height: f32,
}

impl ContentBox {
    /// Контентная область элемента с заданным прямоугольником и внутренним
    /// отступом (`padding` + толщина рамки).
    pub fn from_rect(rect: Rect, inset: f32) -> Self {
        Self {
            left: rect.x + inset,
            top: rect.y + inset,
            width: (rect.w - 2.0 * inset).max(0.0),
            height: (rect.h - 2.0 * inset).max(0.0),
        }
    }
}

/// Метрики текста, необходимые для раскладки.
///
/// Отрисовщик предоставляет реальные метрики шрифта; в тестах — заглушку
/// с фиксированной шириной знака.
pub trait TextMetrics {
    /// Размер строки `text` при заданном размере шрифта: (ширина, высота).
    fn measure(&self, text: &str, font_size: f32) -> (f32, f32);
}

/// Узел разложенного дерева.
#[derive(Debug, Clone)]
pub struct PlacedNode {
    /// Стабильный ключ узла.
    pub key: &'static str,
    /// Тип элемента (с текущими данными).
    pub kind: NodeKind,
    /// Стиль узла (размеры, отступы, цвета, рамка).
    pub style: Style,
    /// Прямоугольник в координатах окна.
    pub rect: Rect,
    /// Индексы детей в плоском списке раскладки.
    pub children: Vec<usize>,
}

impl PlacedNode {
    /// Интерактивен ли узел (кнопка, поле или элемент формы).
    pub fn is_interactive(&self) -> bool {
        matches!(
            self.kind,
            NodeKind::Button { .. }
                | NodeKind::TextField { .. }
                | NodeKind::Checkbox { .. }
                | NodeKind::Radio { .. }
                | NodeKind::Select { .. }
        )
    }
}

/// Унаследованный контекст стиля: разрешённые размер и цвет шрифта родителя.
#[derive(Debug, Clone, Copy)]
struct Inherited {
    /// Разрешённый размер шрифта.
    size: f32,
    /// Разрешённый цвет шрифта.
    color: Option<u32>,
}

/// Разложенное дерево: плоский список узлов в порядке отрисовки
/// (предок раньше потомков, поздний сосед выше раннего).
#[derive(Debug, Clone)]
pub struct Layout {
    nodes: Vec<PlacedNode>,
}

impl Layout {
    /// Раскладывает дерево `root` в область `viewport` размерами `metrics`.
    ///
    /// Корневой узел всегда занимает весь viewport; его содержимое отступает
    /// на собственный `padding`/`border-size` (по умолчанию `0`).
    pub fn build(root: &VElement, viewport: (f32, f32), metrics: &dyn TextMetrics) -> Self {
        let mut layout = Self { nodes: Vec::new() };
        let view = Rect::new(0.0, 0.0, viewport.0, viewport.1);
        layout.place(
            root,
            Point::new(0.0, 0.0),
            ContentBox {
                left: 0.0,
                top: 0.0,
                width: viewport.0,
                height: viewport.1,
            },
            None,
            metrics,
            Inherited {
                size: FONT_SIZE,
                color: None,
            },
        );
        // Корень занимает всё окно независимо от содержимого.
        if let Some(node) = layout.nodes.first_mut() {
            node.rect = view;
        }
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
        if !node.rect.contains(point) {
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

    fn hit(&self, index: usize, point: Point) -> Option<&PlacedNode> {
        let node = &self.nodes[index];
        if !node.rect.contains(point) {
            return None;
        }
        for &child in node.children.iter().rev() {
            if let Some(found) = self.hit(child, point) {
                return Some(found);
            }
        }
        if node.is_interactive() && !is_disabled(&node.kind) {
            Some(node)
        } else {
            None
        }
    }

    /// Помещает узел в поток в точке `(x, y)` и раскладывает его детей.
    ///
    /// `parent_content` — контентная область родителя: от неё отсчитываются
    /// проценты размеров (`width`/`height`/`min-*`/`max-*`). `margin` даёт
    /// внешний отступ узла, `padding`/`border-size` — внутренний.
    ///
    /// `parent` — индекс родительского узла в плоском списке (`None` для корня).
    /// `inherited` — унаследованный размер и цвет шрифта.
    ///
    /// Дети позиционируются относительно **итогового** content box узла (см.
    /// [`Self::align_children`]), а не относительно окна или доступного слота.
    /// Возвращает внешний размер узла (вместе с `margin`) — шаг для соседей.
    fn place(
        &mut self,
        vnode: &VElement,
        origin: Point,
        parent_content: ContentBox,
        parent: Option<usize>,
        metrics: &dyn TextMetrics,
        inherited: Inherited,
    ) -> (f32, f32) {
        let style = vnode.style;
        if !style.visible {
            return (0.0, 0.0);
        }
        let inset = style.inset();
        let margin = style.margin;
        let flow_x = origin.x + margin;
        let flow_y = origin.y + margin;
        let font_size = style.font_size.unwrap_or(inherited.size);
        let font_color = style.font_color.or(inherited.color);
        // Проценты размера отсчитываются от content box родителя.
        let avail_w = parent_content.width;
        let avail_h = parent_content.height;
        // Полный шаг узла по осям (включая внешние отступы) — для родителей.
        let step = 2.0 * margin;
        // Поддерево узла занимает в плоском списке диапазон `[start, len)`.
        let start = self.nodes.len();
        // Корень и окно занимают всю доступную область, поэтому их проценты
        // отсчитываются от неё; остальные — от собственного размера (`Auto` по
        // содержимому => 0, как у неопределённой высоты в CSS).
        let root = parent.is_none() || matches!(vnode.kind, NodeKind::Window);
        let definite_w = if root {
            avail_w
        } else {
            style.resolve_width(0.0, avail_w)
        };
        let definite_h = if root {
            avail_h
        } else {
            style.resolve_height(0.0, avail_h)
        };
        // Контентная область, которую узел отдаёт детям под размеры-проценты.
        let child_content = ContentBox {
            left: flow_x + inset,
            top: flow_y + inset,
            width: (definite_w - 2.0 * inset).max(0.0),
            height: (definite_h - 2.0 * inset).max(0.0),
        };

        let (w, h) = match &vnode.kind {
            NodeKind::Text { content } => {
                let (content_w, content_h) = metrics.measure(content, font_size);
                let w = style.resolve_width(content_w + 2.0 * inset, avail_w);
                let h = style.resolve_height(content_h + 2.0 * inset, avail_h);
                self.push(vnode, Rect::new(flow_x, flow_y, w, h), parent);
                (w, h)
            }
            NodeKind::Button { .. } => {
                let index = self.push(vnode, Rect::default(), parent);
                // Внутренняя область кнопки: собственный `padding`/рамка плюс
                // дефолтные отступы кнопки.
                let inner_left = flow_x + inset + BUTTON_PAD_X;
                let inner_top = flow_y + inset + BUTTON_PAD_Y;
                let inner_w = (definite_w - 2.0 * (inset + BUTTON_PAD_X)).max(0.0);
                let inner_h = (definite_h - 2.0 * (inset + BUTTON_PAD_Y)).max(0.0);
                let content = ContentBox {
                    left: inner_left,
                    top: inner_top,
                    width: inner_w,
                    height: inner_h,
                };
                // Содержимое выкладывается горизонтально (как в `Row`).
                let mut cursor_x = inner_left;
                let mut content_w = 0.0_f32;
                let mut content_h = 0.0_f32;
                let mut any_child = false;
                for child in &vnode.children {
                    if !child.style.visible {
                        continue;
                    }
                    let (child_w, child_h) = self.place(
                        child,
                        Point::new(cursor_x, inner_top),
                        content,
                        Some(index),
                        metrics,
                        Inherited {
                            size: font_size,
                            color: font_color,
                        },
                    );
                    cursor_x += child_w + SPACING;
                    content_h = content_h.max(child_h);
                    any_child = true;
                }
                if any_child {
                    content_w = cursor_x - inner_left - SPACING;
                }
                let w = style.resolve_width(content_w + 2.0 * (inset + BUTTON_PAD_X), avail_w);
                let h = style.resolve_height(
                    (content_h + 2.0 * (inset + BUTTON_PAD_Y)).max(BUTTON_MIN_H),
                    avail_h,
                );
                let node = &mut self.nodes[index];
                node.rect = Rect::new(flow_x, flow_y, w, h);
                // Содержимое кнопки центрируется по обеим осям (группа целиком).
                if any_child {
                    let pad_box = ContentBox::from_rect(node.rect, inset + BUTTON_PAD_X);
                    let first = self.nodes[index].children[0];
                    let dx = pad_box.left + ((pad_box.width - content_w) / 2.0).max(0.0)
                        - self.nodes[first].rect.x;
                    let dy = pad_box.top + ((pad_box.height - content_h) / 2.0).max(0.0)
                        - self.nodes[first].rect.y;
                    if dx != 0.0 || dy != 0.0 {
                        for child in &mut self.nodes[index + 1..] {
                            child.rect.x += dx;
                            child.rect.y += dy;
                        }
                    }
                }
                (w, h)
            }
            NodeKind::TextField { multiline, .. } => {
                let default_h = if *multiline {
                    FIELD_MIN_H + 2.0 * FIELD_LINE_H
                } else {
                    FIELD_MIN_H
                };
                let w = style.resolve_width(FIELD_WIDTH + 2.0 * inset, avail_w);
                let h = style.resolve_height(default_h + 2.0 * inset, avail_h);
                self.push(vnode, Rect::new(flow_x, flow_y, w, h), parent);
                (w, h)
            }
            NodeKind::Checkbox { label, .. } | NodeKind::Radio { label, .. } => {
                let (label_w, label_h) = metrics.measure(label, font_size);
                let content_w = CHECKBOX_BOX + CHECKBOX_GAP + label_w;
                let content_h = CHECKBOX_BOX.max(label_h);
                let w = style.resolve_width(content_w + 2.0 * inset, avail_w);
                let h = style.resolve_height(content_h + 2.0 * inset, avail_h);
                self.push(vnode, Rect::new(flow_x, flow_y, w, h), parent);
                (w, h)
            }
            NodeKind::Select { options, .. } => {
                let widest = options
                    .iter()
                    .map(|option| metrics.measure(&option.label, font_size).0)
                    .fold(0.0_f32, f32::max);
                let content_w = SELECT_WIDTH.max(widest + 2.0 * SELECT_PAD_X + CHECKBOX_BOX);
                let w = style.resolve_width(content_w + 2.0 * inset, avail_w);
                let h = style.resolve_height(FIELD_MIN_H + 2.0 * inset, avail_h);
                self.push(vnode, Rect::new(flow_x, flow_y, w, h), parent);
                (w, h)
            }
            NodeKind::Column | NodeKind::Row | NodeKind::TitleBar | NodeKind::Window => {
                let index = self.push(vnode, Rect::default(), parent);
                let horizontal = matches!(vnode.kind, NodeKind::Row | NodeKind::TitleBar);
                // Контентная область, которую узел отдаёт детям под проценты:
                // собственный определённый размер минус внутренние отступы.
                let mut cursor_x = child_content.left;
                let mut cursor_y = child_content.top;
                let mut content_w = 0.0_f32;
                let mut content_h = 0.0_f32;
                for child in &vnode.children {
                    if !child.style.visible {
                        continue;
                    }
                    let (child_w, child_h) = self.place(
                        child,
                        Point::new(cursor_x, cursor_y),
                        child_content,
                        Some(index),
                        metrics,
                        Inherited {
                            size: font_size,
                            color: font_color,
                        },
                    );
                    if horizontal {
                        cursor_x += child_w + SPACING;
                        content_h = content_h.max(child_h);
                    } else {
                        cursor_y += child_h + SPACING;
                        content_w = content_w.max(child_w);
                    }
                }
                if !vnode.children.is_empty() {
                    if horizontal {
                        content_w = cursor_x - child_content.left - SPACING;
                    } else {
                        content_h = cursor_y - child_content.top - SPACING;
                    }
                }
                let (w, h) = if root {
                    (avail_w, avail_h)
                } else {
                    (
                        style.resolve_width(content_w + 2.0 * inset, avail_w),
                        style.resolve_height(content_h + 2.0 * inset, avail_h),
                    )
                };
                let node = &mut self.nodes[index];
                node.rect = Rect::new(flow_x, flow_y, w, h);
                // Выравниваем прямых детей относительно итогового content box.
                let content = ContentBox::from_rect(node.rect, inset);
                self.align_children(index, content);
                (w, h)
            }
        };

        // Записываем разрешённый размер/цвет шрифта, чтобы отрисовщик и
        // наследование стилей (голый текст в контейнерах) работали корректно.
        self.nodes[start].style.font_size = Some(font_size);
        if let Some(color) = font_color
            && self.nodes[start].style.font_color.is_none()
        {
            self.nodes[start].style.font_color = Some(color);
        }
        (w + step, h + step)
    }

    /// Выравнивает прямых детей узла `index` по `position-x`/`position-y` и
    /// `position-offset-*` относительно его контентной области `content`.
    ///
    /// Абсолютные размеры и координаты фиксированы до вызова, поэтому всё
    /// поддерево ребёнка смещается одной дельтой — относительные позиции
    /// внутри него (в том числе у внуков) сохраняются.
    fn align_children(&mut self, index: usize, content: ContentBox) {
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

    fn push(&mut self, vnode: &VElement, rect: Rect, parent: Option<usize>) -> usize {
        self.nodes.push(PlacedNode {
            key: vnode.key,
            kind: vnode.kind.clone(),
            style: vnode.style,
            rect,
            children: Vec::new(),
        });
        let index = self.nodes.len() - 1;
        if let Some(parent) = parent {
            self.nodes[parent].children.push(index);
        }
        index
    }
}

/// Заблокирован ли узел директивой `:disabled`.
fn is_disabled(kind: &NodeKind) -> bool {
    matches!(
        kind,
        NodeKind::Button { disabled: true, .. }
            | NodeKind::TextField { disabled: true, .. }
            | NodeKind::Checkbox { disabled: true, .. }
            | NodeKind::Radio { disabled: true, .. }
            | NodeKind::Select { disabled: true, .. }
    )
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
fn aligned(
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
