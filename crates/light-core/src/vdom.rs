//! Виртуальное дерево интерфейса и его сравнение.
//!
//! Каждая отрисовка компонента строит новое дерево [`VElement`]; функция
//! [`diff`] сравнивает его с предыдущим и возвращает список точечных
//! изменений ([`Op`]). Неизменившиеся узлы не попадают ни в один оп —
//! именно это обеспечивает перерисовку только изменившихся элементов.

use std::collections::{BTreeSet, HashMap, HashSet};

use crate::collection::Collection;
use crate::component::WindowConfig;
use crate::control::WindowCommand;
use std::sync::{Mutex, OnceLock};

/// Интернирует динамический ключ узла и возвращает стабильную ссылку.
///
/// Используется директивами `v-for`, где ключ строится во время выполнения
/// (базовый путь плюс индекс элемента). Одинаковые строки получают один и тот
/// же `&'static str`, поэтому сопоставление узлов между отрисовками остаётся
/// корректным. Набор ключей ограничен позициями шаблона и максимальной
/// длиной списка.
pub fn intern_key(key: &str) -> &'static str {
    fn pool() -> &'static Mutex<HashMap<String, &'static str>> {
        static POOL: OnceLock<Mutex<HashMap<String, &'static str>>> = OnceLock::new();
        POOL.get_or_init(|| Mutex::new(HashMap::new()))
    }
    let mut pool = pool()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(existing) = pool.get(key) {
        return existing;
    }
    let leaked: &'static str = Box::leak(key.to_owned().into_boxed_str());
    pool.insert(leaked.to_owned(), leaked);
    leaked
}

/// Кнопка мыши, связанная с событием нажатия или отпускания.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    /// Левая кнопка.
    Left,
    /// Правая кнопка.
    Right,
    /// Средняя кнопка.
    Middle,
}

/// Данные о клавише, связанные с событием `@keydown`/`@keyup`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyInfo {
    /// Текстовая подпись клавиши (например, `"a"`, `"Enter"`, `"ArrowLeft"`).
    pub key: String,
    /// Зажата ли клавиша Shift.
    pub shift: bool,
    /// Зажата ли клавиша Ctrl (Control).
    pub ctrl: bool,
    /// Зажата ли клавиша Alt.
    pub alt: bool,
}

impl KeyInfo {
    /// Создаёт данные о клавише с указанными модификаторами.
    pub fn new(key: impl Into<String>, shift: bool, ctrl: bool, alt: bool) -> Self {
        Self {
            key: key.into(),
            shift,
            ctrl,
            alt,
        }
    }
}

/// Событие, которое бэкенд доставляет узлу по его ключу.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// Клик по кнопке.
    Click,
    /// Двойной клик.
    Dblclick,
    /// Нажатие кнопки мыши (не левой — обрабатывается как клик/меню).
    Mousedown(MouseButton),
    /// Отпускание кнопки мыши.
    Mouseup(MouseButton),
    /// Курсор вошёл в пределы элемента.
    Mouseenter,
    /// Курсор покинул пределы элемента.
    Mouseleave,
    /// Перемещение курсора над элементом (координаты относительно узла).
    Mousemove {
        /// Координата X относительно левого-верхнего угла узла.
        x: f32,
        /// Координата Y относительно левого-верхнего угла узла.
        y: f32,
    },
    /// Нажатие клавиши клавиатуры.
    Keydown(KeyInfo),
    /// Отпускание клавиши клавиатуры.
    Keyup(KeyInfo),
    /// Текстовое поле ввело новое значение целиком.
    TextChange(String),
}

/// Размер узла по одной оси (атрибуты `width`/`height`/`left`/`top`).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Size {
    /// Размер по содержимому (значение по умолчанию).
    #[default]
    Auto,
    /// Абсолютный размер в логических пикселях.
    Px(f32),
    /// Размер в процентах от родительского элемента (`0`–`100`).
    Percent(f32),
}

impl Size {
    /// Разрешает размер относительно родителя: `available` — доступный размер
    /// по этой оси, `content` — размер по содержимому (для [`Size::Auto`]).
    pub fn resolve(self, content: f32, available: f32) -> f32 {
        let value = match self {
            Size::Auto => content,
            Size::Px(value) => value,
            Size::Percent(percent) => available * percent / 100.0,
        };
        value.max(0.0)
    }
}

/// Выравнивание узла внутри родителя по оси (`position-x`, `position-y`).
///
/// Для `position-x` значения `left`/`right`/`center` отображаются на
/// [`Align::Start`]/[`Align::End`]/[`Align::Center`]; для `position-y` так же
/// `top`/`bottom`/`center`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Align {
    /// Прижать к началу оси (`left` для X, `top` для Y).
    #[default]
    Start,
    /// Центрировать по оси.
    Center,
    /// Прижать к концу оси (`right` для X, `bottom` для Y).
    End,
}

/// Смещение узла относительно его выравнивания
/// (`position-offset-x`, `position-offset-y`).
///
/// В отличие от [`Size`] допускает отрицательные значения.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Offset {
    /// Смещение в пикселях (может быть отрицательным).
    Px(f32),
    /// Смещение в процентах от размера родителя по оси (может быть
    /// отрицательным).
    Percent(f32),
}

impl Offset {
    /// Разрешает смещение относительно доступного размера `available`.
    pub fn resolve(self, available: f32) -> f32 {
        match self {
            Offset::Px(value) => value,
            Offset::Percent(percent) => available * percent / 100.0,
        }
    }
}

impl Default for Offset {
    fn default() -> Self {
        Offset::Px(0.0)
    }
}

/// Отступы по четырём сторонам (`padding-*`, `margin-*`).
///
/// Значения — в логических пикселях. Атрибут без суффикса (`padding`/`margin`)
/// задаёт сразу все четыре стороны; атрибуты с суффиксом
/// (`padding-top`, `margin-left`, …) переопределяют отдельные стороны.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Edges {
    /// Отступ сверху (`*-top`).
    pub top: f32,
    /// Отступ справа (`*-right`).
    pub right: f32,
    /// Отступ снизу (`*-bottom`).
    pub bottom: f32,
    /// Отступ слева (`*-left`).
    pub left: f32,
}

impl Edges {
    /// Нулевые отступы со всех сторон.
    pub const ZERO: Self = Self {
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
        left: 0.0,
    };

    /// Одинаковый отступ со всех сторон.
    pub const fn all(value: f32) -> Self {
        Self {
            top: value,
            right: value,
            bottom: value,
            left: value,
        }
    }

    /// Суммарный отступ по горизонтали (`left` + `right`).
    pub const fn horizontal(self) -> f32 {
        self.left + self.right
    }

    /// Суммарный отступ по вертикали (`top` + `bottom`).
    pub const fn vertical(self) -> f32 {
        self.top + self.bottom
    }

    /// Все ли стороны нулевые.
    pub const fn is_zero(self) -> bool {
        self.top == 0.0 && self.right == 0.0 && self.bottom == 0.0 && self.left == 0.0
    }

    /// Масштабирует каждую сторону на `factor` (для HiDPI).
    pub const fn scaled(self, factor: f32) -> Self {
        Self {
            top: self.top * factor,
            right: self.right * factor,
            bottom: self.bottom * factor,
            left: self.left * factor,
        }
    }

    /// Покомпонентная сумма двух наборов отступов.
    pub const fn plus(self, other: Self) -> Self {
        Self {
            top: self.top + other.top,
            right: self.right + other.right,
            bottom: self.bottom + other.bottom,
            left: self.left + other.left,
        }
    }
}

/// Возможность изменения размера текстового поля (`resize`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Resize {
    /// Размер фиксирован.
    #[default]
    None,
    /// Можно менять только ширину.
    Horizontal,
    /// Можно менять только высоту.
    Vertical,
    /// Можно менять ширину и высоту.
    Both,
}

/// Тип кнопки (`type`) и её поведение при клике внутри формы.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ButtonType {
    /// Отправка формы (по умолчанию): запускает валидацию `required`.
    #[default]
    Submit,
    /// Обычная кнопка: валидация не запускается.
    Button,
    /// Сброс формы: валидация не запускается.
    Reset,
}

/// Форма указателя мыши (`cursor`).
///
/// Используется как значение атрибута `cursor`/`:cursor` и как результат
/// автоматического выбора курсора для узла (см. `light::window`). Имена
/// соответствуют CSS-значениям: `pointer`, `not-allowed`, `ew-resize` и т. д.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Cursor {
    /// Системный курсор по умолчанию (`default`).
    #[default]
    Default,
    /// Указатель на ссылку или кнопку (`pointer`) — «рука».
    Pointer,
    /// Ввод текста (`text`) — I-beam.
    Text,
    /// Действие запрещено (`not-allowed`).
    NotAllowed,
    /// Ожидание (`wait`).
    Wait,
    /// Идёт фоновая работа, но взаимодействие доступно (`progress`).
    Progress,
    /// Доступна справка (`help`).
    Help,
    /// Перекрестие для выбора области (`crosshair`).
    Crosshair,
    /// Объект можно перемещать (`move`).
    Move,
    /// Объект можно захватить (`grab`).
    Grab,
    /// Объект захвачен (`grabbing`).
    Grabbing,
    /// Изменение ширины (`ew-resize`).
    EwResize,
    /// Изменение высоты (`ns-resize`).
    NsResize,
    /// Изменение по диагонали ↗↙ (`nesw-resize`).
    NeswResize,
    /// Изменение по диагонали ↘↖ (`nwse-resize`).
    NwseResize,
}

impl Cursor {
    /// Разбирает значение атрибута `cursor` (регистр не важен).
    ///
    /// Неизвестное значение возвращает `None` — вызывающая сторона сама решает,
    /// использовать ли курсор по роли элемента.
    pub fn from_name(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "default" => Some(Self::Default),
            "pointer" => Some(Self::Pointer),
            "text" => Some(Self::Text),
            "not-allowed" => Some(Self::NotAllowed),
            "wait" => Some(Self::Wait),
            "progress" => Some(Self::Progress),
            "help" => Some(Self::Help),
            "crosshair" => Some(Self::Crosshair),
            "move" => Some(Self::Move),
            "grab" => Some(Self::Grab),
            "grabbing" => Some(Self::Grabbing),
            "ew-resize" => Some(Self::EwResize),
            "ns-resize" => Some(Self::NsResize),
            "nesw-resize" => Some(Self::NeswResize),
            "nwse-resize" => Some(Self::NwseResize),
            _ => None,
        }
    }
}

/// Визуальный индикатор обязательного поля (`required-indicator`).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum RequiredIndicator {
    /// Красная звёздочка `*` (по умолчанию): после подписи у флажка/радио/списка
    /// или в правом верхнем углу текстового поля.
    #[default]
    Asterisk,
    /// Индикатор не рисуется — разработчик добавляет пометку сам.
    None,
    /// Акцентная полоса слева от поля заданным цветом (`0xRRGGBBAA`).
    Accent(u32),
}

fn size_or_auto_impl(value: &str) -> Option<Size> {
    let value = value.trim();
    if value.is_empty() || value.eq_ignore_ascii_case("auto") {
        return Some(Size::Auto);
    }
    if let Some(percent) = value.strip_suffix('%') {
        return percent.trim().parse::<f32>().ok().map(Size::Percent);
    }
    let number = value.strip_suffix("px").unwrap_or(value).trim();
    number.parse::<f32>().ok().map(Size::Px)
}

/// Разбирает значение размера (`width`/`height`) во время выполнения.
///
/// Правила совпадают со временем компиляции шаблонов: `auto`/пусто —
/// [`Size::Auto`], `50%` — [`Size::Percent`], `120` или `120px` — [`Size::Px`].
/// Некорректное значение в debug-сборке вызывает `debug_assert!`, а в release
/// возвращает [`Size::Auto`].
pub fn parse_size_or_auto(value: &str) -> Size {
    size_or_auto_impl(value).unwrap_or_else(|| {
        debug_assert!(false, "invalid size value: \"{value}\"");
        Size::Auto
    })
}

fn offset_impl(value: &str) -> Option<Offset> {
    let value = value.trim();
    if let Some(percent) = value.strip_suffix('%') {
        return percent.trim().parse::<f32>().ok().map(Offset::Percent);
    }
    let number = value.strip_suffix("px").unwrap_or(value).trim();
    number.parse::<f32>().ok().map(Offset::Px)
}

/// Разбирает значение смещения (`position-offset-x`/`position-offset-y`) во
/// время выполнения: `-8` и `12px` — [`Offset::Px`], `10%` — [`Offset::Percent`]
/// (знак допускается). Некорректное значение в debug-сборке вызывает
/// `debug_assert!`, а в release возвращает [`Offset::default`].
pub fn parse_offset(value: &str) -> Offset {
    offset_impl(value).unwrap_or_else(|| {
        debug_assert!(false, "invalid offset value: \"{value}\"");
        Offset::default()
    })
}

fn align_impl(value: &str, start: &str, end: &str) -> Option<Align> {
    match value.trim().to_ascii_lowercase().as_str() {
        value if value == start => Some(Align::Start),
        "center" => Some(Align::Center),
        value if value == end => Some(Align::End),
        _ => None,
    }
}

/// Разбирает `position-x` (`left`/`center`/`right`). Некорректное значение в
/// debug-сборке вызывает `debug_assert!`, а в release возвращает
/// [`Align::Start`].
pub fn parse_align_x(value: &str) -> Align {
    align_impl(value, "left", "right").unwrap_or_else(|| {
        debug_assert!(false, "invalid position-x value: \"{value}\"");
        Align::Start
    })
}

/// Разбирает `position-y` (`top`/`center`/`bottom`). Некорректное значение в
/// debug-сборке вызывает `debug_assert!`, а в release возвращает
/// [`Align::Start`].
pub fn parse_align_y(value: &str) -> Align {
    align_impl(value, "top", "bottom").unwrap_or_else(|| {
        debug_assert!(false, "invalid position-y value: \"{value}\"");
        Align::Start
    })
}

/// Разбирает `justify-content` (`start`/`center`/`end`). Некорректное значение в
/// debug-сборке вызывает `debug_assert!`, а в release возвращает
/// [`Align::Start`].
pub fn parse_justify(value: &str) -> Align {
    align_impl(value, "start", "end").unwrap_or_else(|| {
        debug_assert!(false, "invalid justify-content value: \"{value}\"");
        Align::Start
    })
}

/// Разбирает `align-items` (`start`/`center`/`end`). Некорректное значение в
/// debug-сборке вызывает `debug_assert!`, а в release возвращает
/// [`Align::Start`].
pub fn parse_align_items(value: &str) -> Align {
    align_impl(value, "start", "end").unwrap_or_else(|| {
        debug_assert!(false, "invalid align-items value: \"{value}\"");
        Align::Start
    })
}

fn px_impl(value: &str) -> Option<f32> {
    let value = value.trim();
    let number = value.strip_suffix("px").unwrap_or(value).trim();
    number.parse::<f32>().ok()
}

/// Разбирает значение в пикселях (`12`, `12px`). Некорректное значение в
/// debug-сборке вызывает `debug_assert!`, а в release возвращает `0.0`.
pub fn parse_px(value: &str) -> f32 {
    px_impl(value).unwrap_or_else(|| {
        debug_assert!(false, "invalid pixel value: \"{value}\"");
        0.0
    })
}

fn color_impl(value: &str) -> Option<u32> {
    let raw = value.trim();
    let digits = raw
        .strip_prefix('#')
        .or_else(|| raw.strip_prefix("0x"))
        .unwrap_or(raw);
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |text: &str| u8::from_str_radix(text, 16).ok();
    let (r, g, b, a) = match digits.len() {
        3 => (
            byte(&digits[0..1])? * 17,
            byte(&digits[1..2])? * 17,
            byte(&digits[2..3])? * 17,
            0xFF,
        ),
        4 => (
            byte(&digits[0..1])? * 17,
            byte(&digits[1..2])? * 17,
            byte(&digits[2..3])? * 17,
            byte(&digits[3..4])? * 17,
        ),
        6 => (
            byte(&digits[0..2])?,
            byte(&digits[2..4])?,
            byte(&digits[4..6])?,
            0xFF,
        ),
        8 => (
            byte(&digits[0..2])?,
            byte(&digits[2..4])?,
            byte(&digits[4..6])?,
            byte(&digits[6..8])?,
        ),
        _ => return None,
    };
    Some(((r as u32) << 24) | ((g as u32) << 16) | ((b as u32) << 8) | a as u32)
}

/// Разбирает цвет (`#RGB`, `#RGBA`, `#RRGGBB`, `#RRGGBBAA`, `0xRRGGBB`,
/// `0xRRGGBBAA`) в `0xRRGGBBAA`. Возвращает `None` для некорректного значения
/// (в debug-сборке дополнительно вызывает `debug_assert!`).
pub fn parse_color(value: &str) -> Option<u32> {
    let parsed = color_impl(value);
    debug_assert!(
        parsed.is_some(),
        "invalid color value: \"{value}\" (#RGB, #RGBA, #RRGGBB, #RRGGBBAA)"
    );
    parsed
}

/// Разбирает булев атрибут: истина для `true`/`1`/`yes`/`on`.
pub fn parse_bool(value: &str) -> bool {
    matches!(value.trim(), "true" | "1" | "yes" | "on")
}

fn required_indicator_impl(value: &str) -> Option<RequiredIndicator> {
    match value.trim() {
        "" | "*" => Some(RequiredIndicator::Asterisk),
        "none" => Some(RequiredIndicator::None),
        other => color_impl(other).map(RequiredIndicator::Accent),
    }
}

/// Разбирает `required-indicator`: `none`, `*` (по умолчанию) или цвет
/// (`#RRGGBB`). Некорректное значение в debug-сборке вызывает `debug_assert!`,
/// а в release возвращает [`RequiredIndicator::Asterisk`].
pub fn parse_required_indicator(value: &str) -> RequiredIndicator {
    required_indicator_impl(value).unwrap_or_else(|| {
        debug_assert!(false, "invalid required-indicator value: \"{value}\"");
        RequiredIndicator::Asterisk
    })
}

fn button_type_impl(value: &str) -> Option<ButtonType> {
    match value.trim().to_ascii_lowercase().as_str() {
        "" | "submit" => Some(ButtonType::Submit),
        "button" => Some(ButtonType::Button),
        "reset" => Some(ButtonType::Reset),
        _ => None,
    }
}

/// Разбирает тип кнопки (`submit`/`button`/`reset`). Некорректное значение в
/// debug-сборке вызывает `debug_assert!`, а в release возвращает
/// [`ButtonType::Submit`].
pub fn parse_button_type(value: &str) -> ButtonType {
    button_type_impl(value).unwrap_or_else(|| {
        debug_assert!(false, "invalid button type value: \"{value}\"");
        ButtonType::Submit
    })
}

fn resize_impl(value: &str) -> Option<Resize> {
    match value.trim().to_ascii_lowercase().as_str() {
        "" | "none" => Some(Resize::None),
        "horizontal" => Some(Resize::Horizontal),
        "vertical" => Some(Resize::Vertical),
        "both" => Some(Resize::Both),
        _ => None,
    }
}

/// Разбирает режим изменения размера поля
/// (`none`/`horizontal`/`vertical`/`both`). Некорректное значение в debug-сборке
/// вызывает `debug_assert!`, а в release возвращает [`Resize::None`].
pub fn parse_resize(value: &str) -> Resize {
    resize_impl(value).unwrap_or_else(|| {
        debug_assert!(false, "invalid resize value: \"{value}\"");
        Resize::None
    })
}

/// Стиль узла: размеры, выравнивание, отступы, цвета и рамка.
///
/// Все размеры — в логических пикселях; `width`/`height` и смещения можно
/// задавать в процентах от родительского элемента (`50%`). По умолчанию
/// `padding`, `margin`, `border-size` и смещения равны `0`, а выравнивание —
/// по началу оси.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Style {
    /// Ширина (`width`).
    pub width: Size,
    /// Высота (`height`).
    pub height: Size,
    /// Минимальная ширина (`min-width`), по умолчанию не ограничена.
    pub min_width: Option<Size>,
    /// Минимальная высота (`min-height`), по умолчанию не ограничена.
    pub min_height: Option<Size>,
    /// Максимальная ширина (`max-width`), по умолчанию не ограничена.
    pub max_width: Option<Size>,
    /// Максимальная высота (`max-height`), по умолчанию не ограничена.
    pub max_height: Option<Size>,
    /// Доля свободного места по main-оси контейнера (`flex-grow`/`flex`).
    /// `0` — не растягиваться, `1` — забрать всё свободное место.
    pub flex_grow: f32,
    /// Доля переполнения по main-оси, которую узел отдаёт при сжатии
    /// (`flex-shrink`/`flex`). `0` — не сжиматься.
    pub flex_shrink: f32,
    /// Выравнивание по горизонтали (`position-x`).
    pub align_x: Align,
    /// Выравнивание по вертикали (`position-y`).
    pub align_y: Align,
    /// Горизонтальное смещение (`position-offset-x`).
    pub offset_x: Offset,
    /// Вертикальное смещение (`position-offset-y`).
    pub offset_y: Offset,
    /// Выравнивание **содержимого** по main-оси контейнера (`justify-content`):
    /// у `Column` — вертикаль, у `Row` — горизонталь. По умолчанию — начало оси.
    pub justify_content: Align,
    /// Выравнивание **содержимого** по cross-оси контейнера (`align-items`):
    /// у `Column` — горизонталь, у `Row` — вертикаль. По умолчанию — начало оси.
    pub align_items: Align,
    /// Внутренние отступы (`padding`/`padding-*`), по умолчанию `0`.
    pub padding: Edges,
    /// Внешние отступы (`margin`/`margin-*`), по умолчанию `0`.
    pub margin: Edges,
    /// Цвет фона (`background`, `0xRRGGBBAA`).
    pub background: Option<u32>,
    /// Цвет текста (`font-color`, `0xRRGGBBAA`).
    pub font_color: Option<u32>,
    /// Размер шрифта (`font-size`, px).
    pub font_size: Option<f32>,
    /// Цвет рамки (`border-color`, `0xRRGGBBAA`).
    pub border_color: Option<u32>,
    /// Толщина рамки (`border-size`, px).
    pub border_size: f32,
    /// Видимость узла (`visible`): скрытые узлы не занимают места в раскладке
    /// и не получают событий.
    pub visible: bool,
    /// Обязательное поле (`required`): рисуется индикатор и запускается
    /// проверка при отправке формы.
    pub required: bool,
    /// Поле не прошло валидацию (`invalid`): рисуется красная рамка,
    /// в дерево доступности попадает `aria-invalid`.
    pub invalid: bool,
    /// Как показывать обязательность поля (`required-indicator`).
    pub required_indicator: RequiredIndicator,
    /// Порядок отрисовки и хит-теста (больше z — выше).
    pub z: i32,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            width: Size::Auto,
            height: Size::Auto,
            min_width: None,
            min_height: None,
            max_width: None,
            max_height: None,
            flex_grow: 0.0,
            flex_shrink: 0.0,
            align_x: Align::Start,
            align_y: Align::Start,
            offset_x: Offset::Px(0.0),
            offset_y: Offset::Px(0.0),
            justify_content: Align::Start,
            align_items: Align::Start,
            padding: Edges::ZERO,
            margin: Edges::ZERO,
            background: None,
            font_color: None,
            font_size: None,
            border_color: None,
            border_size: 0.0,
            visible: true,
            required: false,
            invalid: false,
            required_indicator: RequiredIndicator::Asterisk,
            z: 0,
        }
    }
}

impl Style {
    /// Отступ содержимого узла от его границ по каждой стороне:
    /// `padding` + `border_size`.
    pub fn inset(&self) -> Edges {
        self.padding.plus(Edges::all(self.border_size))
    }

    /// Разрешает ширину с учётом `min-width`/`max-width`.
    pub fn resolve_width(&self, content: f32, available: f32) -> f32 {
        let mut value = self.width.resolve(content, available);
        if let Some(min) = self.min_width {
            value = value.max(min.resolve(content, available));
        }
        if let Some(max) = self.max_width {
            value = value.min(max.resolve(content, available));
        }
        value.max(0.0)
    }

    /// Разрешает высоту с учётом `min-height`/`max-height`.
    pub fn resolve_height(&self, content: f32, available: f32) -> f32 {
        let mut value = self.height.resolve(content, available);
        if let Some(min) = self.min_height {
            value = value.max(min.resolve(content, available));
        }
        if let Some(max) = self.max_height {
            value = value.min(max.resolve(content, available));
        }
        value.max(0.0)
    }
}

/// Вариант выпадающего списка (`<option>` внутри `<select>`).
#[derive(Debug, Clone, PartialEq)]
pub struct SelectOption {
    /// Отображаемая подпись.
    pub label: String,
    /// Значение, записываемое в `v-model` при выборе.
    pub value: String,
}

/// Тип элемента интерфейса.
///
/// Тип узла статичен: он задаётся позицией в шаблоне и не меняется между
/// отрисовками (структурные изменения — это появление/исчезновение узла).
#[derive(Debug, Clone, PartialEq)]
pub enum NodeKind {
    /// Контейнер, раскладывающий детей вертикально (`<div>` по умолчанию).
    Column,
    /// Контейнер, раскладывающий детей горизонтально (`direction="horizontal"`).
    Row,
    /// Титульная полоса окна (`<titlebar>`): горизонтальный контейнер, за
    /// который перетаскивается безрамочное окно. Его текст задаёт заголовок.
    TitleBar,
    /// Корневой узел окна (`<window>`). Контейнер клиентской области; несёт
    /// настройки системного окна в поле [`VElement::window`].
    Window,
    /// Текстовый узел.
    Text {
        /// Отображаемый текст.
        content: String,
    },
    /// Кнопка-контейнер: содержимое (`children`) выкладывается горизонтально и
    /// центрируется по обеим осям. Подпись собирается из текста детей.
    Button {
        /// Заблокирована ли кнопка (`:disabled`).
        disabled: bool,
        /// Тип кнопки (`type`): влияет на запуск валидации формы.
        button_type: ButtonType,
    },
    /// Текстовое поле (`<text>`); при `multiline` — многострочное
    /// (аналог HTML `<textarea>`).
    TextField {
        /// Текущее значение (`v-model`).
        value: String,
        /// Подсказка, когда значение пусто (`placeholder`).
        placeholder: String,
        /// Заблокировано ли поле (`:disabled`).
        disabled: bool,
        /// Многострочный режим (`textarea`).
        multiline: bool,
        /// Возможность изменения размера (`resize`).
        resize: Resize,
        /// Скрывать ли вводимый текст (`<password>`): значение отображается
        /// точками, но хранится как есть.
        masked: bool,
    },
    /// Флажок (`<checkbox>`): переключает булево значение `v-model`.
    #[cfg(feature = "elements-checkbox")]
    Checkbox {
        /// Подпись рядом с флажком.
        label: String,
        /// Отмечен ли флажок (`v-model`).
        checked: bool,
        /// Заблокирован ли флажок (`:disabled`).
        disabled: bool,
    },
    /// Переключатель (`<radio>`): выбирает одно значение из группы,
    /// объединённой общим `v-model`.
    #[cfg(feature = "elements-radio")]
    Radio {
        /// Подпись рядом с переключателем.
        label: String,
        /// Значение, записываемое в `v-model` при выборе.
        value: String,
        /// Совпадает ли текущее значение `v-model` с полем `value` варианта
        /// [`NodeKind::Radio`].
        selected: bool,
        /// Заблокирован ли переключатель (`:disabled`).
        disabled: bool,
    },
    /// Выпадающий список (`<select>`).
    #[cfg(feature = "elements-select")]
    Select {
        /// Текущее выбранное значение (`v-model`).
        value: String,
        /// Варианты списка (`<option>`).
        options: Vec<SelectOption>,
        /// Заблокирован ли список (`:disabled`).
        disabled: bool,
    },
}

/// Узел виртуального дерева.
///
/// `key` — стабильный путь к узлу от корня (назначается макросом по позиции
/// в шаблоне): он переживает между отрисовками и служит идентичностью узла
/// при сравнении деревьев.
#[derive(Debug, Clone, PartialEq)]
pub struct VElement {
    /// Стабильный путь-идентификатор узла (`""` — корень).
    pub key: &'static str,
    /// Тип элемента.
    pub kind: NodeKind,
    /// Размеры узла из атрибутов `width`/`height`.
    pub style: Style,
    /// Имя HTML-тега источника (`"div"`, `"button"`, …). Для программно
    /// созданных узлов и текстовых сегментов — пустая строка.
    pub tag: &'static str,
    /// Атрибуты элемента `(имя, значение)` без директив и обработчиков.
    ///
    /// Динамические привязки (`:disabled="locked"`) хранятся под именем без
    /// двоеточия со строковым значением на момент отрисовки.
    pub attrs: Vec<(&'static str, String)>,
    /// Настройки системного окна — только у корневого узла с типом
    /// [`NodeKind::Window`]; у остальных узлов — `None`.
    pub window: Option<WindowConfig>,
    /// Дочерние узлы (пусто для текста и поля).
    pub children: Vec<VElement>,
}

impl VElement {
    /// Создаёт узел с пустым списком детей.
    pub fn leaf(key: &'static str, kind: NodeKind) -> Self {
        Self {
            key,
            kind,
            style: Style::default(),
            tag: "",
            attrs: Vec::new(),
            window: None,
            children: Vec::new(),
        }
    }

    /// Создаёт контейнер с детьми.
    pub fn container(key: &'static str, kind: NodeKind, children: Vec<VElement>) -> Self {
        Self {
            key,
            kind,
            style: Style::default(),
            tag: "",
            attrs: Vec::new(),
            window: None,
            children,
        }
    }

    /// Задаёт размеры узла (атрибуты `width`/`height`).
    pub fn with_style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// Задаёт настройки системного окна (для корневого узла [`NodeKind::Window`]).
    pub fn with_window(mut self, config: WindowConfig) -> Self {
        self.window = Some(config);
        self
    }

    /// Настройки системного окна, если узел — окно.
    pub fn window_config(&self) -> Option<&WindowConfig> {
        self.window.as_ref()
    }

    /// Задаёт имя HTML-тега источника.
    pub fn with_tag(mut self, tag: &'static str) -> Self {
        self.tag = tag;
        self
    }

    /// Добавляет один атрибут элемента.
    pub fn with_attr(mut self, name: &'static str, value: impl Into<String>) -> Self {
        self.attrs.push((name, value.into()));
        self
    }

    /// Задаёт список атрибутов элемента.
    pub fn with_attrs(mut self, attrs: Vec<(&'static str, String)>) -> Self {
        self.attrs = attrs;
        self
    }

    /// Значение атрибута по имени, если он задан.
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| value.as_str())
    }

    /// Ищет узел по ключу (обход в глубину).
    pub fn get(&self, key: &str) -> Option<&VElement> {
        if self.key == key {
            return Some(self);
        }
        self.children.iter().find_map(|child| child.get(key))
    }

    /// Первый узел поддерева с указанным именем HTML-тега.
    pub fn find_by_tag(&self, tag: &str) -> Option<&VElement> {
        if self.tag == tag {
            return Some(self);
        }
        self.children
            .iter()
            .find_map(|child| child.find_by_tag(tag))
    }

    /// Первый узел поддерева с атрибутом `name`, равным `value`.
    pub fn find_by_attr(&self, name: &str, value: &str) -> Option<&VElement> {
        if self.attr(name) == Some(value) {
            return Some(self);
        }
        self.children
            .iter()
            .find_map(|child| child.find_by_attr(name, value))
    }

    /// Ищет узел по ключу для изменения (обход в глубину).
    pub fn get_mut(&mut self, key: &str) -> Option<&mut VElement> {
        if self.key == key {
            return Some(self);
        }
        self.children
            .iter_mut()
            .find_map(|child| child.get_mut(key))
    }

    /// Текстовое содержимое узла для поиска (только у текстовых узлов).
    pub fn content(&self) -> Option<&str> {
        match &self.kind {
            NodeKind::Text { content } => Some(content),
            _ => None,
        }
    }

    /// Все узлы поддерева (включая сам узел) в порядке обхода в глубину.
    ///
    /// Возвращается [`Collection`], поэтому результат можно сразу фильтровать
    /// и преобразовывать цепочкой методов.
    pub fn descendants(&self) -> Collection<&VElement> {
        let mut nodes = Collection::new();
        self.collect_descendants(&mut nodes);
        nodes
    }

    fn collect_descendants<'a>(&'a self, out: &mut Collection<&'a VElement>) {
        out.push(self);
        for child in &self.children {
            child.collect_descendants(out);
        }
    }

    /// Ищет первый узел с заданным текстовым содержимым.
    pub fn find_by_content(&self, content: &str) -> Option<&VElement> {
        if self.content() == Some(content) {
            return Some(self);
        }
        self.children
            .iter()
            .find_map(|child| child.find_by_content(content))
    }

    /// Ищет первый узел, содержимое которого содержит подстроку `needle`.
    pub fn find_containing(&self, needle: &str) -> Option<&VElement> {
        if self
            .content()
            .is_some_and(|content| content.contains(needle))
        {
            return Some(self);
        }
        self.children
            .iter()
            .find_map(|child| child.find_containing(needle))
    }

    /// Первый узел титульной полосы (`<titlebar>`), если он есть.
    fn find_titlebar(&self) -> Option<&VElement> {
        if matches!(self.kind, NodeKind::TitleBar) {
            return Some(self);
        }
        self.children.iter().find_map(VElement::find_titlebar)
    }

    /// Собирает простой текст поддерева, пропуская интерактивные элементы.
    fn collect_plain_text(&self, out: &mut String) {
        match &self.kind {
            NodeKind::Text { content } => out.push_str(content),
            NodeKind::Button { .. } | NodeKind::TextField { .. } => {}
            #[cfg(feature = "elements-checkbox")]
            NodeKind::Checkbox { .. } => {}
            #[cfg(feature = "elements-radio")]
            NodeKind::Radio { .. } => {}
            #[cfg(feature = "elements-select")]
            NodeKind::Select { .. } => {}
            _ => {
                for child in &self.children {
                    child.collect_plain_text(out);
                }
            }
        }
    }

    /// Заголовок окна, выведенный из текста первого `<titlebar>`.
    ///
    /// Интерактивные элементы (кнопки управления) в заголовок не попадают.
    /// Возвращает `None`, если титульной полосы нет или её текст пуст.
    pub fn titlebar_text(&self) -> Option<String> {
        let bar = self.find_titlebar()?;
        let mut text = String::new();
        bar.collect_plain_text(&mut text);
        let trimmed = text.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_owned())
        }
    }
}

/// Общий интерфейс любого элемента дерева: чтение и изменение свойств плюс
/// отправка действий оконному бэкенду.
///
/// Это «базовый класс» для узлов интерфейса: конкретный элемент — [`VElement`],
/// который реализует этот трейт. Операции над окном описаны в [`VWindow`].
pub trait VNode {
    /// Стабильный ключ узла.
    fn key(&self) -> &'static str;
    /// Тип элемента.
    fn kind(&self) -> &NodeKind;
    /// Изменяемый тип элемента.
    fn kind_mut(&mut self) -> &mut NodeKind;
    /// Стиль узла.
    fn style(&self) -> &Style;
    /// Изменяемый стиль узла.
    fn style_mut(&mut self) -> &mut Style;
    /// Имя HTML-тега источника.
    fn tag(&self) -> &'static str;
    /// Атрибуты элемента.
    fn attrs(&self) -> &[(&'static str, String)];
    /// Дочерние узлы.
    fn children(&self) -> &[VElement];

    /// Значение атрибута по имени.
    fn attr(&self, name: &str) -> Option<&str> {
        self.attrs()
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| value.as_str())
    }

    /// Текст узла (для [`NodeKind::Text`]).
    fn text(&self) -> Option<&str> {
        match self.kind() {
            NodeKind::Text { content } => Some(content),
            _ => None,
        }
    }

    /// Задаёт текст узла (для [`NodeKind::Text`]).
    fn set_text(&mut self, value: impl Into<String>) {
        if let NodeKind::Text { content } = self.kind_mut() {
            *content = value.into();
        }
    }

    /// Виден ли узел.
    fn visible(&self) -> bool {
        self.style().visible
    }

    /// Показывает или скрывает узел.
    fn set_visible(&mut self, value: bool) {
        self.style_mut().visible = value;
    }

    /// Заблокирован ли узел.
    fn disabled(&self) -> bool {
        match self.kind() {
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

    /// Блокирует или разблокирует узел.
    fn set_disabled(&mut self, value: bool) {
        match self.kind_mut() {
            NodeKind::Button { disabled, .. } | NodeKind::TextField { disabled, .. } => {
                *disabled = value;
            }
            #[cfg(feature = "elements-checkbox")]
            NodeKind::Checkbox { disabled, .. } => *disabled = value,
            #[cfg(feature = "elements-radio")]
            NodeKind::Radio { disabled, .. } => *disabled = value,
            #[cfg(feature = "elements-select")]
            NodeKind::Select { disabled, .. } => *disabled = value,
            _ => {}
        }
    }

    /// Текущее значение (поля ввода или списка).
    fn value(&self) -> Option<&str> {
        match self.kind() {
            NodeKind::TextField { value, .. } => Some(value),
            #[cfg(feature = "elements-select")]
            NodeKind::Select { value, .. } => Some(value),
            _ => None,
        }
    }

    /// Задаёт текущее значение.
    fn set_value(&mut self, new_value: impl Into<String>) {
        let new_value = new_value.into();
        match self.kind_mut() {
            NodeKind::TextField { value, .. } => {
                *value = new_value;
            }
            #[cfg(feature = "elements-select")]
            NodeKind::Select { value, .. } => {
                *value = new_value;
            }
            _ => {}
        }
    }

    /// Отмечен ли флажок.
    fn checked(&self) -> bool {
        #[cfg(feature = "elements-checkbox")]
        {
            matches!(self.kind(), NodeKind::Checkbox { checked: true, .. })
        }
        #[cfg(not(feature = "elements-checkbox"))]
        {
            false
        }
    }

    /// Ставит или снимает флажок.
    fn set_checked(&mut self, value: bool) {
        #[cfg(feature = "elements-checkbox")]
        if let NodeKind::Checkbox { checked, .. } = self.kind_mut() {
            *checked = value;
        }
        #[cfg(not(feature = "elements-checkbox"))]
        let _ = value;
    }

    /// Выбран ли переключатель.
    fn selected(&self) -> bool {
        #[cfg(feature = "elements-radio")]
        {
            matches!(self.kind(), NodeKind::Radio { selected: true, .. })
        }
        #[cfg(not(feature = "elements-radio"))]
        {
            false
        }
    }

    /// Выбирает или снимает переключатель.
    fn set_selected(&mut self, value: bool) {
        #[cfg(feature = "elements-radio")]
        if let NodeKind::Radio { selected, .. } = self.kind_mut() {
            *selected = value;
        }
        #[cfg(not(feature = "elements-radio"))]
        let _ = value;
    }

    /// Варианты выпадающего списка.
    fn options(&self) -> &[SelectOption] {
        match self.kind() {
            #[cfg(feature = "elements-select")]
            NodeKind::Select { options, .. } => options,
            _ => &[],
        }
    }

    /// Задаёт варианты выпадающего списка.
    fn set_options(&mut self, value: Vec<SelectOption>) {
        #[cfg(feature = "elements-select")]
        if let NodeKind::Select { options, .. } = self.kind_mut() {
            *options = value;
        }
        #[cfg(not(feature = "elements-select"))]
        let _ = value;
    }

    /// Отправляет действие оконному бэкенду для окна, событие которого
    /// обрабатывается в данный момент.
    fn act(&self, command: WindowCommand) {
        crate::control::request(command);
    }
}

/// Операции над окном — расширение интерфейса узла [`VNode`].
///
/// Реализуется корневым узлом [`NodeKind::Window`]; методы отправляют команду
/// активному окну (см. [`crate::control`]).
pub trait VWindow: VNode {
    /// Настройки системного окна, если узел — окно.
    fn config(&self) -> Option<&WindowConfig>;

    /// Закрывает окно.
    fn close(&self) {
        self.act(WindowCommand::Close);
    }

    /// Сворачивает окно.
    fn minimize(&self) {
        self.act(WindowCommand::Minimize);
    }

    /// Разворачивает окно.
    fn maximize(&self) {
        self.act(WindowCommand::Maximize);
    }

    /// Возвращает окно в обычный размер.
    fn restore(&self) {
        self.act(WindowCommand::Restore);
    }

    /// Переключает развёрнутость окна.
    fn toggle_maximize(&self) {
        self.act(WindowCommand::ToggleMaximize);
    }

    /// Показывает скрытое окно.
    fn show(&self) {
        self.act(WindowCommand::Show);
    }

    /// Скрывает окно.
    fn hide(&self) {
        self.act(WindowCommand::Hide);
    }

    /// Открывает дочернее модальное окно поверх текущего.
    fn open_modal(&self) {
        self.act(WindowCommand::OpenModal);
    }

    /// Открывает ещё одно главное окно приложения.
    fn open_window(&self) {
        self.act(WindowCommand::OpenWindow);
    }

    /// Открывает (или фокусирует) окно с идентификатором `id`.
    fn open(&self, id: &str) {
        self.act(WindowCommand::Open(id.to_owned()));
    }

    /// Закрывает окно с идентификатором `id` вместе с потомками.
    fn close_window(&self, id: &str) {
        self.act(WindowCommand::CloseWindow(id.to_owned()));
    }

    /// Показывает окно с идентификатором `id`.
    fn show_window(&self, id: &str) {
        self.act(WindowCommand::ShowWindow(id.to_owned()));
    }

    /// Скрывает окно с идентификатором `id`.
    fn hide_window(&self, id: &str) {
        self.act(WindowCommand::HideWindow(id.to_owned()));
    }

    /// Развёрнуто ли окно в данный момент.
    fn is_maximized(&self) -> bool {
        crate::control::is_maximized()
    }
}

impl VNode for VElement {
    fn key(&self) -> &'static str {
        self.key
    }

    fn kind(&self) -> &NodeKind {
        &self.kind
    }

    fn kind_mut(&mut self) -> &mut NodeKind {
        &mut self.kind
    }

    fn style(&self) -> &Style {
        &self.style
    }

    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }

    fn tag(&self) -> &'static str {
        self.tag
    }

    fn attrs(&self) -> &[(&'static str, String)] {
        &self.attrs
    }

    fn children(&self) -> &[VElement] {
        &self.children
    }
}

impl VWindow for VElement {
    fn config(&self) -> Option<&WindowConfig> {
        self.window.as_ref()
    }
}

/// Точечное изменение одного узла между отрисовками.
#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    /// Изменилось содержимое текстового узла.
    SetContent {
        /// Ключ узла.
        key: &'static str,
        /// Новый текст.
        content: String,
    },
    /// Изменилась подпись кнопки.
    SetLabel {
        /// Ключ узла.
        key: &'static str,
        /// Новая подпись.
        label: String,
    },
    /// Изменился флаг disabled (кнопки или поля).
    SetDisabled {
        /// Ключ узла.
        key: &'static str,
        /// Новое значение.
        disabled: bool,
    },
    /// Изменилось значение текстового поля.
    SetValue {
        /// Ключ узла.
        key: &'static str,
        /// Новое значение.
        value: String,
    },
    /// Изменился placeholder текстового поля.
    SetPlaceholder {
        /// Ключ узла.
        key: &'static str,
        /// Новый placeholder.
        placeholder: String,
    },
    /// Изменился режим скрытия текста (`<password>`).
    SetMasked {
        /// Ключ узла.
        key: &'static str,
        /// Скрывать ли текст.
        masked: bool,
    },
    /// Изменилось состояние флажка.
    SetChecked {
        /// Ключ узла.
        key: &'static str,
        /// Новое состояние.
        checked: bool,
    },
    /// Изменилось состояние переключателя.
    SetSelected {
        /// Ключ узла.
        key: &'static str,
        /// Новое состояние.
        selected: bool,
    },
    /// Изменился набор пунктов выпадающего списка.
    SetOptions {
        /// Ключ узла.
        key: &'static str,
        /// Новые пункты.
        options: Vec<SelectOption>,
    },
    /// Узел добавлен вместе с поддеревом.
    Insert {
        /// Ключ узла.
        key: &'static str,
    },
    /// Узел удалён вместе с поддеревом.
    Remove {
        /// Ключ узла.
        key: &'static str,
    },
    /// Тип узла несовместим с прежним — узел заменён целиком.
    Replace {
        /// Ключ узла.
        key: &'static str,
    },
}

impl Op {
    /// Ключ затронутого узла.
    pub fn key(&self) -> &'static str {
        match self {
            Self::SetContent { key, .. }
            | Self::SetLabel { key, .. }
            | Self::SetDisabled { key, .. }
            | Self::SetValue { key, .. }
            | Self::SetPlaceholder { key, .. }
            | Self::SetMasked { key, .. }
            | Self::SetChecked { key, .. }
            | Self::SetSelected { key, .. }
            | Self::SetOptions { key, .. }
            | Self::Insert { key }
            | Self::Remove { key }
            | Self::Replace { key } => key,
        }
    }
}

/// Результат сравнения двух деревьев.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Patch {
    /// Список точечных изменений в порядке обхода.
    pub ops: Vec<Op>,
    /// Ключи узлов, содержимое которых изменилось (для перерисовки их областей).
    pub changed: BTreeSet<&'static str>,
}

impl Patch {
    /// Истина, если изменений нет.
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }
}

/// Сравнивает новое дерево со старым и возвращает точечные изменения.
///
/// Узлы сопоставляются по стабильным ключам: изменившиеся свойства дают
/// операции `Set*`, добавленные/удалённые — `Insert`/`Remove`. Узлы, у которых
/// ничего не поменялось, не порождают никаких операций.
pub fn diff(old: &VElement, new: &VElement) -> Patch {
    let mut patch = Patch::default();
    diff_node(old, new, &mut patch);
    patch
}

fn diff_node(old: &VElement, new: &VElement, patch: &mut Patch) {
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
fn kind_compatible(a: &NodeKind, b: &NodeKind) -> bool {
    match (a, b) {
        (NodeKind::Column, NodeKind::Column)
        | (NodeKind::Row, NodeKind::Row)
        | (NodeKind::TitleBar, NodeKind::TitleBar)
        | (NodeKind::Window, NodeKind::Window)
        | (NodeKind::Text { .. }, NodeKind::Text { .. })
        | (NodeKind::Button { .. }, NodeKind::Button { .. })
        | (NodeKind::TextField { .. }, NodeKind::TextField { .. }) => true,
        #[cfg(feature = "elements-checkbox")]
        (NodeKind::Checkbox { .. }, NodeKind::Checkbox { .. }) => true,
        #[cfg(feature = "elements-radio")]
        (NodeKind::Radio { .. }, NodeKind::Radio { .. }) => true,
        #[cfg(feature = "elements-select")]
        (NodeKind::Select { .. }, NodeKind::Select { .. }) => true,
        _ => false,
    }
}

fn diff_kind(old: &NodeKind, new: &NodeKind, key: &'static str, patch: &mut Patch) {
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
        _ => debug_assert!(false, "несовместимые типы должны фильтроваться раньше"),
    }
}

fn diff_children(old: &VElement, new: &VElement, patch: &mut Patch) {
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
