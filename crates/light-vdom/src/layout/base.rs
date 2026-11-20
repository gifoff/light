//! Раскладка виртуального дерева в прямоугольники, поиск узлов по точке
//! и вычисление областей перерисовки.

use std::{cell::RefCell, collections::HashMap};

use crate::vdom::{Edges, NodeKind, Overflow, Style, VElement, WhiteSpace};

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
pub(crate) fn button_extra_padding(vnode: &VElement, style: &Style) -> Edges {
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

/// Заголовок `h1`–`h5` — это контейнер-колонка с тегом `h1`–`h5`.
/// Его ширина остаётся `auto` (max-content), поэтому при родителе с
/// `align-items != start` (`flow.stretch == None`) колонка получает базовую
/// ширину `0`, и внутренний текст, растянутый на `100%` родительской
/// контентной ширины, обнуляется вместе с ним.
pub(crate) fn is_heading(vnode: &VElement) -> bool {
    matches!(vnode.kind, NodeKind::Column) && matches!(vnode.tag, "h1" | "h2" | "h3" | "h4" | "h5")
}

/// Ось `cross`-растяжения потомков в потоке родителя.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Axis {
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
pub(crate) struct FlowContext {
    /// Контентная область родителя.
    pub content: ContentBox,
    /// Известна ли ширина контентной области (не `Auto`).
    pub width_known: bool,
    /// Известна ли высота контентной области (не `Auto`).
    pub height_known: bool,
    /// Ось, по которой родитель растягивает потомков с `Auto` размером.
    pub stretch: Option<Axis>,
    /// Принудительная ширина от `flex-grow`/`flex-shrink` (внутренняя, без
    /// внешних отступов); переопределяет `width`.
    pub forced_w: Option<f32>,
    /// Принудительная высота от `flex-grow`/`flex-shrink` (внутренняя, без
    /// внешних отступов); переопределяет `height`.
    pub forced_h: Option<f32>,
}

/// Метрики текста, необходимые для раскладки.
///
/// Отрисовщик предоставляет реальные метрики шрифта; в тестах — заглушку
/// с фиксированной шириной знака. Высота должна быть высотой типографской
/// строки без `line-gap` (`ascent - descent`), чтобы выравнивание текста
/// совпадало с видимыми глифами.
pub trait TextMetrics {
    /// Ширина и высота строки `text` при заданном размере шрифта: (ширина, высота).
    fn measure(&self, text: &str, font_size: f32) -> (f32, f32);
}

/// Кэш измерений на время одной раскладки.
///
/// `flex-grow` раскладывает контейнер дважды, поэтому одни и те же строки
/// измеряются повторно. Ключ — адрес, длина и размер шрифта: строки принадлежат
/// неизменяемому дереву, которое живёт дольше раскладки.
type TextMeasureCache = RefCell<HashMap<(usize, usize, u32), (String, (f32, f32))>>;

pub(crate) struct MeasuredText<'a> {
    pub inner: &'a dyn TextMetrics,
    pub cache: TextMeasureCache,
}

impl<'a> MeasuredText<'a> {
    pub(crate) fn new(inner: &'a dyn TextMetrics) -> Self {
        Self {
            inner,
            cache: RefCell::new(HashMap::new()),
        }
    }
}

impl TextMetrics for MeasuredText<'_> {
    fn measure(&self, text: &str, font_size: f32) -> (f32, f32) {
        let key = (text.as_ptr() as usize, text.len(), font_size.to_bits());
        if let Some((cached, size)) = self.cache.borrow().get(&key)
            && cached == text
        {
            return *size;
        }
        let size = self.inner.measure(text, font_size);
        self.cache.borrow_mut().insert(key, (text.to_owned(), size));
        size
    }
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
pub(crate) struct Inherited {
    /// Разрешённый размер шрифта.
    pub size: f32,
    /// Разрешённый цвет шрифта.
    pub color: Option<u32>,
    /// Разрешённый режим переноса текста.
    pub white_space: WhiteSpace,
}

/// Разложенное дерево: плоский список узлов в порядке отрисовки
/// (предок раньше потомков; поздний сосед выше раннего; среди соседей выше
/// тот, у кого больше `z`).
#[derive(Debug, Clone)]
pub struct Layout {
    /// Плоский список узлов в порядке отрисовки.
    pub nodes: Vec<PlacedNode>,
}
