//! Раскладка виртуального дерева в прямоугольники, поиск узлов по точке
//! и вычисление областей перерисовки.

use std::collections::{BTreeSet, HashMap};

#[cfg(debug_assertions)]
use std::env;

use crate::vdom::{
    Align, Edges, NodeKind, Offset, Overflow, Position, Resize, Size, Style, VElement, WhiteSpace,
};

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

    /// Пересечение двух прямоугольников; пустой прямоугольник при отсутствии
    /// пересечения.
    pub fn intersection(self, other: Self) -> Self {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let right = (self.x + self.w).min(other.x + other.w);
        let bottom = (self.y + self.h).min(other.y + other.h);
        Self::new(x, y, (right - x).max(0.0), (bottom - y).max(0.0))
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
    /// Контентная область элемента с заданным прямоугольником и внутренними
    /// отступами по каждой стороне (`padding-*` + толщина рамки).
    pub fn from_rect(rect: Rect, inset: Edges) -> Self {
        Self {
            left: rect.x + inset.left,
            top: rect.y + inset.top,
            width: (rect.w - inset.left - inset.right).max(0.0),
            height: (rect.h - inset.top - inset.bottom).max(0.0),
        }
    }
}

/// Встроенные (по умолчанию) отступы кнопки по сторонам.
///
/// Значения [`BUTTON_PAD_X`]/[`BUTTON_PAD_Y`] действуют, пока пользователь не
/// задал какой-либо `padding`/`padding-*` (статически, через `:`-привязку или
/// программно). Явный отступ переопределяет встроенные — тогда возвращается
/// нулевой набор: собственный `padding` уже учтён в [`Style::inset`].
fn button_extra_padding(vnode: &VElement, style: &Style) -> Edges {
    let explicit = vnode.attr("padding").is_some()
        || vnode.attr("padding-top").is_some()
        || vnode.attr("padding-right").is_some()
        || vnode.attr("padding-bottom").is_some()
        || vnode.attr("padding-left").is_some()
        || !style.padding.is_zero();
    if explicit {
        Edges::ZERO
    } else {
        Edges {
            top: BUTTON_PAD_Y,
            right: BUTTON_PAD_X,
            bottom: BUTTON_PAD_Y,
            left: BUTTON_PAD_X,
        }
    }
}

/// Ось `cross`-растяжения потомков в потоке родителя.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Axis {
    /// Горизонтальная ось (растяжение по ширине).
    Width,
    /// Вертикальная ось (растяжение по высоте).
    Height,
}

/// Контекст потока родителя для размещения ребёнка.
///
/// Содержит контентную область родителя (базу процентов), признак того, что
/// размер родителя по оси определён (`Auto`-размер не даёт базы), и ось
/// `cross`, по которой родитель растягивает потомков с `Auto` размером.
#[derive(Debug, Clone, Copy)]
struct FlowContext {
    /// Контентная область родителя.
    content: ContentBox,
    /// Известна ли ширина контентной области (не `Auto`).
    width_known: bool,
    /// Известна ли высота контентной области (не `Auto`).
    height_known: bool,
    /// Ось, по которой родитель растягивает потомков с `Auto` размером.
    stretch: Option<Axis>,
    /// Принудительная ширина от `flex-grow`/`flex-shrink` (внутренняя, без
    /// внешних отступов); переопределяет `width`.
    forced_w: Option<f32>,
    /// Принудительная высота от `flex-grow`/`flex-shrink` (внутренняя, без
    /// внешних отступов); переопределяет `height`.
    forced_h: Option<f32>,
}

/// Метрики текста, необходимые для раскладки.
///
/// Отрисовщик предоставляет реальные метрики шрифта; в тестах — заглушку
/// с фиксированной шириной знака. Высота должна быть высотой типографской
/// строки без `line-gap` (`ascent - descent`), чтобы выравнивание текста
/// совпадало с видимыми глифами.
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
    /// Режим переполнения по горизонтали.
    pub overflow_x: Overflow,
    /// Режим переполнения по вертикали.
    pub overflow_y: Overflow,
    /// Полная ширина содержимого (включая прокручиваемую часть).
    pub scroll_width: f32,
    /// Полная высота содержимого (включая прокручиваемую часть).
    pub scroll_height: f32,
    /// Текущее горизонтальное смещение прокрутки.
    pub scroll_x: f32,
    /// Текущее вертикальное смещение прокрутки.
    pub scroll_y: f32,
    /// Обрезка, унаследованная от прокручиваемых предков. Собственное
    /// переполнение узла в этот прямоугольник не входит.
    pub clip_rect: Option<Rect>,
}

impl PlacedNode {
    /// Content box узла: прямоугольник внутри рамки и внутренних отступов.
    pub fn content_rect(&self) -> Rect {
        let inset = self.style.inset();
        Rect::new(
            self.rect.x + inset.left,
            self.rect.y + inset.top,
            (self.rect.w - inset.horizontal()).max(0.0),
            (self.rect.h - inset.vertical()).max(0.0),
        )
    }

    /// Максимальное горизонтальное смещение прокрутки.
    pub fn max_scroll_x(&self) -> f32 {
        (self.scroll_width - self.rect.w).max(0.0)
    }

    /// Максимальное вертикальное смещение прокрутки.
    pub fn max_scroll_y(&self) -> f32 {
        (self.scroll_height - self.rect.h).max(0.0)
    }

    /// Интерактивен ли узел (кнопка, поле или элемент формы).
    pub fn is_interactive(&self) -> bool {
        match self.kind {
            NodeKind::Button { .. } | NodeKind::TextField { .. } => true,
            #[cfg(feature = "elements-checkbox")]
            NodeKind::Checkbox { .. } => true,
            #[cfg(feature = "elements-radio")]
            NodeKind::Radio { .. } => true,
            #[cfg(feature = "elements-select")]
            NodeKind::Select { .. } => true,
            _ => false,
        }
    }
}

/// Унаследованный контекст стиля: разрешённые размер и цвет шрифта родителя.
#[derive(Debug, Clone, Copy)]
struct Inherited {
    /// Разрешённый размер шрифта.
    size: f32,
    /// Разрешённый цвет шрифта.
    color: Option<u32>,
    /// Разрешённый режим переноса текста.
    white_space: WhiteSpace,
}

/// Разложенное дерево: плоский список узлов в порядке отрисовки
/// (предок раньше потомков; поздний сосед выше раннего; среди соседей выше
/// тот, у кого больше `z`).
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
            metrics,
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

    fn apply_clip_and_shift(&mut self, index: usize, end: usize) {
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

    fn hit(&self, index: usize, point: Point) -> Option<&PlacedNode> {
        let node = &self.nodes[index];
        if !node.rect.contains(point) || node.clip_rect.is_some_and(|clip| !clip.contains(point)) {
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

    /// Помещает узел в поток в точке `(origin.x, origin.y)` и раскладывает его
    /// детей.
    ///
    /// `flow` — контекст потока родителя: контентная область (база процентов),
    /// известность размеров и ось `cross`-растяжения. `margin` даёт внешний
    /// отступ узла, `padding`/`border-size` — внутренний.
    ///
    /// `parent` — индекс родительского узла в плоском списке (`None` для корня).
    /// `inherited` — унаследованный размер и цвет шрифта.
    ///
    /// Контейнер с `Auto` размером по `cross`-оси родителя **растягивается** на
    /// доступную ширину/высоту (как block/`stretch` во flexbox); листья и кнопки
    /// — по содержимому. Дети позиционируются относительно **итогового** content
    /// box узла (см. [`Self::align_children`]), а не относительно окна.
    /// Возвращает внешний размер узла (вместе с `margin`) — шаг для соседей.
    fn place(
        &mut self,
        vnode: &VElement,
        origin: Point,
        flow: FlowContext,
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
        let flow_x = origin.x + margin.left;
        let flow_y = origin.y + margin.top;
        let font_size = style.font_size.unwrap_or(inherited.size);
        let font_color = style.font_color.or(inherited.color);
        let white_space = style.white_space.unwrap_or(inherited.white_space);
        // Проценты размера отсчитываются от контентной области родителя.
        let avail_w = flow.content.width;
        let avail_h = flow.content.height;
        // Полный шаг узла по осям (включая внешние отступы) — для родителей.
        let step_w = margin.left + margin.right;
        let step_h = margin.top + margin.bottom;
        // Поддерево узла занимает в плоском списке диапазон `[start, len)`.
        let start = self.nodes.len();
        // Корень и окно занимают всю доступную область. Контейнер с `Auto`
        // размером по `cross`-оси родителя растягивается на доступный размер;
        // иначе `Auto` даёт базу `0` (как неопределённая высота в CSS).
        let root = parent.is_none() || matches!(vnode.kind, NodeKind::Window);
        let is_container = matches!(
            vnode.kind,
            NodeKind::Column | NodeKind::Row | NodeKind::TitleBar | NodeKind::Window
        );
        let stretch_w = is_container && flow.stretch == Some(Axis::Width);
        let stretch_h = is_container && flow.stretch == Some(Axis::Height);
        let definite_w = flow.forced_w.unwrap_or(if root {
            avail_w
        } else if matches!(style.width, Size::Auto) {
            if stretch_w { avail_w } else { 0.0 }
        } else {
            style.resolve_width(0.0, avail_w)
        });
        let definite_h = flow.forced_h.unwrap_or(if root {
            avail_h
        } else if matches!(style.height, Size::Auto) {
            if stretch_h { avail_h } else { 0.0 }
        } else {
            style.resolve_height(0.0, avail_h)
        });
        // Определён ли размер узла по оси (для процентов его детей).
        let width_known = flow.forced_w.is_some()
            || root
            || matches!(style.width, Size::Px(_))
            || (matches!(style.width, Size::Percent(_)) && flow.width_known)
            || (matches!(style.width, Size::Auto) && stretch_w);
        let height_known = flow.forced_h.is_some()
            || root
            || matches!(style.height, Size::Px(_))
            || (matches!(style.height, Size::Percent(_)) && flow.height_known)
            || (matches!(style.height, Size::Auto) && stretch_h);
        // Контентная область, которую узел отдаёт детям под размеры-проценты.
        let child_content = ContentBox {
            left: flow_x + inset.left,
            top: flow_y + inset.top,
            width: (definite_w - inset.left - inset.right).max(0.0),
            height: (definite_h - inset.top - inset.bottom).max(0.0),
        };
        // Ось, по которой узел растягивает своих детей (если размер известен).
        let child_stretch = match vnode.kind {
            NodeKind::Row | NodeKind::TitleBar if height_known => Some(Axis::Height),
            NodeKind::Column | NodeKind::Window if width_known => Some(Axis::Width),
            _ => None,
        };
        let child_flow = FlowContext {
            content: child_content,
            width_known,
            height_known,
            stretch: child_stretch,
            forced_w: None,
            forced_h: None,
        };

        let (w, h) = match &vnode.kind {
            NodeKind::Text { content } => {
                let explicit_w = flow.forced_w.or_else(|| {
                    (!matches!(style.width, Size::Auto)).then(|| style.resolve_width(0.0, avail_w))
                });
                let limit = if let Some(width) = explicit_w {
                    width
                } else if let Some(max) = style.max_width {
                    max.resolve(0.0, avail_w)
                } else if flow.width_known {
                    avail_w
                } else {
                    f32::INFINITY
                };
                let limit = if let Some(max) = style.max_width {
                    limit.min(max.resolve(0.0, avail_w))
                } else {
                    limit
                };
                let limit = (limit - inset.horizontal()).max(1.0);
                let (content_w, content_h) = match white_space {
                    WhiteSpace::Normal => {
                        let lines = crate::text_wrap::wrap_words(content, limit, |line| {
                            metrics.measure(line, font_size).0
                        });
                        let width = lines
                            .iter()
                            .map(|line| metrics.measure(line, font_size).0)
                            .fold(0.0, f32::max);
                        let row_h = metrics.measure("", font_size).1.max(1.0);
                        (width, lines.len() as f32 * row_h)
                    }
                    WhiteSpace::Nowrap => metrics.measure(content, font_size),
                };
                let w = flow.forced_w.unwrap_or_else(|| {
                    style.resolve_width(content_w + inset.horizontal(), avail_w)
                });
                let h = flow
                    .forced_h
                    .unwrap_or_else(|| style.resolve_height(content_h + inset.vertical(), avail_h));
                self.push(vnode, Rect::new(flow_x, flow_y, w, h), parent);
                (w, h)
            }
            #[cfg(feature = "image-support")]
            NodeKind::Image { width, height, .. } => {
                let intrinsic_w = *width as f32;
                let intrinsic_h = *height as f32;
                let explicit_w = flow.forced_w.or_else(|| {
                    (!matches!(style.width, Size::Auto))
                        .then(|| style.resolve_width(intrinsic_w, avail_w))
                });
                let explicit_h = flow.forced_h.or_else(|| {
                    (!matches!(style.height, Size::Auto))
                        .then(|| style.resolve_height(intrinsic_h, avail_h))
                });
                let (w, h) = match (explicit_w, explicit_h) {
                    (Some(w), Some(h)) => (w, h),
                    (Some(w), None) => {
                        let inner_w = (w - inset.horizontal()).max(0.0);
                        let inner_h = if intrinsic_w > 0.0 {
                            inner_w * intrinsic_h / intrinsic_w
                        } else {
                            0.0
                        };
                        (w, inner_h + inset.vertical())
                    }
                    (None, Some(h)) => {
                        let inner_h = (h - inset.vertical()).max(0.0);
                        let inner_w = if intrinsic_h > 0.0 {
                            inner_h * intrinsic_w / intrinsic_h
                        } else {
                            0.0
                        };
                        (inner_w + inset.horizontal(), h)
                    }
                    (None, None) => (
                        intrinsic_w + inset.horizontal(),
                        intrinsic_h + inset.vertical(),
                    ),
                };
                self.push(vnode, Rect::new(flow_x, flow_y, w, h), parent);
                (w, h)
            }
            #[cfg(feature = "image-support")]
            NodeKind::RuntimeImage { image, .. } => {
                let intrinsic_w = image.width() as f32;
                let intrinsic_h = image.height() as f32;
                let explicit_w = flow.forced_w.or_else(|| {
                    (!matches!(style.width, Size::Auto))
                        .then(|| style.resolve_width(intrinsic_w, avail_w))
                });
                let explicit_h = flow.forced_h.or_else(|| {
                    (!matches!(style.height, Size::Auto))
                        .then(|| style.resolve_height(intrinsic_h, avail_h))
                });
                let (w, h) = match (explicit_w, explicit_h) {
                    (Some(w), Some(h)) => (w, h),
                    (Some(w), None) => {
                        let inner_w = (w - inset.horizontal()).max(0.0);
                        let inner_h = if intrinsic_w > 0.0 {
                            inner_w * intrinsic_h / intrinsic_w
                        } else {
                            0.0
                        };
                        (w, inner_h + inset.vertical())
                    }
                    (None, Some(h)) => {
                        let inner_h = (h - inset.vertical()).max(0.0);
                        let inner_w = if intrinsic_h > 0.0 {
                            inner_h * intrinsic_w / intrinsic_h
                        } else {
                            0.0
                        };
                        (inner_w + inset.horizontal(), h)
                    }
                    (None, None) => (
                        intrinsic_w + inset.horizontal(),
                        intrinsic_h + inset.vertical(),
                    ),
                };
                self.push(vnode, Rect::new(flow_x, flow_y, w, h), parent);
                (w, h)
            }
            NodeKind::Button { .. } => {
                let index = self.push(vnode, Rect::default(), parent);
                // Внутренняя область кнопки: собственная рамка и `padding`.
                // Встроенные отступы кнопки (14/9) — значения по умолчанию,
                // которые переопределяет заданный пользователем `padding`.
                let extra = button_extra_padding(vnode, &style);
                let btn_inset = inset.plus(extra);
                let inner_left = flow_x + btn_inset.left;
                let inner_top = flow_y + btn_inset.top;
                let inner_w = (definite_w - btn_inset.horizontal()).max(0.0);
                let inner_h = (definite_h - btn_inset.vertical()).max(0.0);
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
                    if !child.style.visible || child.style.position == Position::Absolute {
                        continue;
                    }
                    let (child_w, child_h) = self.place(
                        child,
                        Point::new(cursor_x, inner_top),
                        FlowContext {
                            content,
                            width_known: inner_w > 0.0,
                            height_known: inner_h > 0.0,
                            stretch: None,
                            forced_w: None,
                            forced_h: None,
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
                    content_h = content_h.max(child_h);
                    any_child = true;
                }
                if any_child {
                    content_w = cursor_x - inner_left - SPACING;
                }
                let w = flow.forced_w.unwrap_or_else(|| {
                    style.resolve_width(content_w + btn_inset.horizontal(), avail_w)
                });
                let h = flow.forced_h.unwrap_or_else(|| {
                    style.resolve_height(
                        (content_h + btn_inset.vertical()).max(BUTTON_MIN_H),
                        avail_h,
                    )
                });
                let node = &mut self.nodes[index];
                node.rect = Rect::new(flow_x, flow_y, w, h);
                #[cfg(debug_assertions)]
                debug_trace(vnode, node.rect, child_flow);
                // Содержимое кнопки центрируется по обеим осям (группа целиком).
                if any_child {
                    let pad_box = ContentBox {
                        left: node.rect.x + btn_inset.left,
                        top: node.rect.y + btn_inset.top,
                        width: (node.rect.w - btn_inset.horizontal()).max(0.0),
                        height: (node.rect.h - btn_inset.vertical()).max(0.0),
                    };
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
                let w = flow.forced_w.unwrap_or_else(|| {
                    style.resolve_width(FIELD_WIDTH + inset.horizontal(), avail_w)
                });
                let h = flow
                    .forced_h
                    .unwrap_or_else(|| style.resolve_height(default_h + inset.vertical(), avail_h));
                self.push(vnode, Rect::new(flow_x, flow_y, w, h), parent);
                (w, h)
            }
            #[cfg(feature = "elements-checkbox")]
            NodeKind::Checkbox { label, .. } => {
                let (label_w, label_h) = metrics.measure(label, font_size);
                let content_w = CHECKBOX_BOX + CHECKBOX_GAP + label_w;
                let content_h = CHECKBOX_BOX.max(label_h);
                let w = flow.forced_w.unwrap_or_else(|| {
                    style.resolve_width(content_w + inset.horizontal(), avail_w)
                });
                let h = flow
                    .forced_h
                    .unwrap_or_else(|| style.resolve_height(content_h + inset.vertical(), avail_h));
                self.push(vnode, Rect::new(flow_x, flow_y, w, h), parent);
                (w, h)
            }
            #[cfg(feature = "elements-radio")]
            NodeKind::Radio { label, .. } => {
                let (label_w, label_h) = metrics.measure(label, font_size);
                let content_w = CHECKBOX_BOX + CHECKBOX_GAP + label_w;
                let content_h = CHECKBOX_BOX.max(label_h);
                let w = flow.forced_w.unwrap_or_else(|| {
                    style.resolve_width(content_w + inset.horizontal(), avail_w)
                });
                let h = flow
                    .forced_h
                    .unwrap_or_else(|| style.resolve_height(content_h + inset.vertical(), avail_h));
                self.push(vnode, Rect::new(flow_x, flow_y, w, h), parent);
                (w, h)
            }
            #[cfg(feature = "elements-select")]
            NodeKind::Select { options, .. } => {
                let widest = options
                    .iter()
                    .map(|option| metrics.measure(&option.label, font_size).0)
                    .fold(0.0_f32, f32::max);
                let content_w = SELECT_WIDTH.max(widest + 2.0 * SELECT_PAD_X + CHECKBOX_BOX);
                let w = flow.forced_w.unwrap_or_else(|| {
                    style.resolve_width(content_w + inset.horizontal(), avail_w)
                });
                let h = flow.forced_h.unwrap_or_else(|| {
                    style.resolve_height(FIELD_MIN_H + inset.vertical(), avail_h)
                });
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
                let mut child_sizes = Vec::with_capacity(vnode.children.len());
                let mut any_flow = false;
                for child in &vnode.children {
                    if !child.style.visible || child.style.position == Position::Absolute {
                        continue;
                    }
                    any_flow = true;
                    let (child_w, child_h) = self.place(
                        child,
                        Point::new(cursor_x, cursor_y),
                        child_flow,
                        Some(index),
                        metrics,
                        Inherited {
                            size: font_size,
                            color: font_color,
                            white_space,
                        },
                    );
                    child_sizes.push((child_w, child_h));
                    if horizontal {
                        cursor_x += child_w + SPACING;
                        content_h = content_h.max(child_h);
                    } else {
                        cursor_y += child_h + SPACING;
                        content_w = content_w.max(child_w);
                    }
                }
                if any_flow {
                    if horizontal {
                        content_w = cursor_x - child_content.left - SPACING;
                    } else {
                        content_h = cursor_y - child_content.top - SPACING;
                    }
                }
                // Второй проход: распределяем свободное место (`flex-grow`) или
                // переполнение (`flex-shrink`) между прямыми детьми по main-оси.
                let main_avail = if horizontal {
                    child_content.width
                } else {
                    child_content.height
                };
                let used_main = if horizontal { content_w } else { content_h };
                let main_known = if horizontal {
                    width_known
                } else {
                    height_known
                };
                let scrolls_main = if horizontal {
                    style.overflow_x.scrollable()
                } else {
                    style.overflow_y.scrollable()
                };
                let raw_free = main_avail - used_main;
                // В контейнере с размером по содержимому нет переполнения,
                // а содержимое scroll-контейнера не сжимаем — иначе пропадёт
                // прокрутка.
                let free = if main_known && (raw_free >= 0.0 || !scrolls_main) {
                    raw_free
                } else {
                    0.0
                };
                let total_flex: f32 = vnode
                    .children
                    .iter()
                    .filter(|child| child.style.visible && child.style.position == Position::Flow)
                    .map(|child| {
                        if free > 0.0 {
                            child.style.flex_grow
                        } else {
                            child.style.flex_shrink
                        }
                    })
                    .sum();
                if free != 0.0 && total_flex > 0.0 {
                    let forced: Vec<(Option<f32>, Option<f32>)> = vnode
                        .children
                        .iter()
                        .filter(|child| {
                            child.style.visible && child.style.position == Position::Flow
                        })
                        .zip(child_sizes.iter())
                        .map(|(child, &(child_w, child_h))| {
                            let weight = if free > 0.0 {
                                child.style.flex_grow
                            } else {
                                child.style.flex_shrink
                            };
                            let extra = free * weight / total_flex;
                            if horizontal {
                                let step = child.style.margin.left + child.style.margin.right;
                                (Some((child_w - step + extra).max(0.0)), None)
                            } else {
                                let step = child.style.margin.top + child.style.margin.bottom;
                                (None, Some((child_h - step + extra).max(0.0)))
                            }
                        })
                        .collect();
                    // Пересобираем поддерево с принудительными main-размерами.
                    self.nodes.truncate(index + 1);
                    self.nodes[index].children.clear();
                    cursor_x = child_content.left;
                    cursor_y = child_content.top;
                    content_w = 0.0;
                    content_h = 0.0;
                    let mut visible = vnode
                        .children
                        .iter()
                        .filter(|child| {
                            child.style.visible && child.style.position == Position::Flow
                        })
                        .zip(forced.iter());
                    for (child, &(forced_w, forced_h)) in &mut visible {
                        let (child_w, child_h) = self.place(
                            child,
                            Point::new(cursor_x, cursor_y),
                            FlowContext {
                                forced_w,
                                forced_h,
                                ..child_flow
                            },
                            Some(index),
                            metrics,
                            Inherited {
                                size: font_size,
                                color: font_color,
                                white_space,
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
                    if any_flow {
                        if horizontal {
                            content_w = cursor_x - child_content.left - SPACING;
                        } else {
                            content_h = cursor_y - child_content.top - SPACING;
                        }
                    }
                }
                let w = flow.forced_w.unwrap_or(if root {
                    avail_w
                } else if stretch_w {
                    style.resolve_width(avail_w, avail_w)
                } else {
                    style.resolve_width(content_w + inset.horizontal(), avail_w)
                });
                let h = flow.forced_h.unwrap_or(if root {
                    avail_h
                } else if stretch_h {
                    style.resolve_height(avail_h, avail_h)
                } else {
                    style.resolve_height(content_h + inset.vertical(), avail_h)
                });
                self.nodes[index].rect = Rect::new(flow_x, flow_y, w, h);
                let content = ContentBox::from_rect(self.nodes[index].rect, inset);
                // В горизонтальном контейнере с высотой по содержимому дети с
                // `height:auto` растягиваются на итоговую высоту после того,
                // как она стала известна (аналог `align-items: stretch`).
                if horizontal && !height_known && style.align_items == Align::Start {
                    (content_w, content_h) = self.stretch_row_children(
                        vnode,
                        index,
                        content,
                        metrics,
                        Inherited {
                            size: font_size,
                            color: font_color,
                            white_space,
                        },
                    );
                }
                // Абсолютные дети не участвуют в потоке и раскладываются после
                // того, как известен итоговый content box родителя.
                for child in &vnode.children {
                    if !child.style.visible || child.style.position != Position::Absolute {
                        continue;
                    }
                    self.place(
                        child,
                        Point::new(content.left, content.top),
                        FlowContext {
                            content,
                            width_known: true,
                            height_known: true,
                            stretch: None,
                            forced_w: None,
                            forced_h: None,
                        },
                        Some(index),
                        metrics,
                        Inherited {
                            size: font_size,
                            color: font_color,
                            white_space,
                        },
                    );
                }
                self.nodes[index].scroll_width = (content_w + inset.horizontal()).max(w);
                self.nodes[index].scroll_height = (content_h + inset.vertical()).max(h);
                let used_main = if horizontal { content_w } else { content_h };
                let cross_attr = if horizontal {
                    "position-y"
                } else {
                    "position-x"
                };
                let explicit_cross: Vec<bool> = vnode
                    .children
                    .iter()
                    .filter(|child| child.style.visible && child.style.position == Position::Flow)
                    .map(|child| child.attr(cross_attr).is_some())
                    .collect();
                self.align_container_content(
                    index,
                    content,
                    horizontal,
                    used_main,
                    &explicit_cross,
                );
                self.align_children(index, content);
                (w, h)
            }
        };

        if !is_container {
            self.nodes[start].scroll_width = w;
            self.nodes[start].scroll_height = h;
        }
        // Записываем разрешённый размер/цвет шрифта, чтобы отрисовщик и
        // наследование стилей (голый текст в контейнерах) работали корректно.
        self.nodes[start].style.font_size = Some(font_size);
        self.nodes[start].style.white_space = Some(white_space);
        if let Some(color) = font_color
            && self.nodes[start].style.font_color.is_none()
        {
            self.nodes[start].style.font_color = Some(color);
        }
        (w + step_w, h + step_h)
    }

    /// Повторно раскладывает детей горизонтального контейнера, растягивая
    /// детей с `height:auto` на итоговую высоту content box.
    fn stretch_row_children(
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

    fn row_content_size(&self, index: usize) -> (f32, f32) {
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
    fn align_container_content(
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

    /// Переупорядочивает плоский список так, чтобы среди соседей порядок
    /// отрисовки и попадания задавался `z`; геометрия узлов не меняется.
    ///
    /// Дети каждого узла стабильно сортируются по `style.z` (равные `z`
    /// сохраняют DOM-порядок), затем список перестраивается в обходе в глубину
    /// по новому порядку детей, а индексы детей перенумеровываются. `z`
    /// действует только среди соседей (поддерево остаётся внутри родителя).
    /// Корень остаётся нулевым индексом.
    fn reorder_by_z(&mut self) {
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

    fn push(&mut self, vnode: &VElement, rect: Rect, parent: Option<usize>) -> usize {
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

/// Заблокирован ли узел директивой `:disabled`.
fn is_disabled(kind: &NodeKind) -> bool {
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

#[cfg(debug_assertions)]
fn debug_trace(vnode: &VElement, rect: Rect, flow: FlowContext) {
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
