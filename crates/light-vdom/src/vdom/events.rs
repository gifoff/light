use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

pub use crate::component::{DEFAULT_BACKGROUND, WindowConfig, background_rgba};

/// Интернирует динамический ключ узла и возвращает стабильную ссылку.
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

/// Режим переполнения контейнера по оси (`overflow`, `overflow-x`,
/// `overflow-y`).
///
/// `Visible` не создаёт прокрутку и не обрезает содержимое. `Hidden` обрезает
/// содержимое по границам контейнера. `Auto` обрезает содержимое и включает
/// прокрутку, если оно не помещается. `Scroll` резервирует полосу прокрутки
/// всегда, но в текущей реализации отрисовывается так же, как `Auto`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Overflow {
    /// Содержимое может выходить за границы контейнера.
    #[default]
    Visible,
    /// Содержимое обрезается по границам контейнера.
    Hidden,
    /// Содержимое обрезается, прокрутка появляется при переполнении.
    Auto,
    /// Содержимое обрезается, полоса прокрутки резервируется всегда.
    Scroll,
}

impl Overflow {
    /// Обрезает ли режим содержимое по границам контейнера.
    pub const fn clips(self) -> bool {
        !matches!(self, Overflow::Visible)
    }

    /// Разрешена ли прокрутка по оси.
    pub const fn scrollable(self) -> bool {
        matches!(self, Overflow::Auto | Overflow::Scroll)
    }
}

/// Режим переноса текста (`white-space`).
///
/// `Normal` переносит текст по словам, если он не помещается в доступную
/// ширину. `NoWrap` оставляет текст одной строкой; при нехватке места он
/// обрезается многоточием.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WhiteSpace {
    /// Переносить текст по словам (значение по умолчанию).
    #[default]
    Normal,
    /// Не переносить текст; переполнение обрезается с `…`.
    Nowrap,
}

/// Участие узла в нормальном потоке (`position`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Position {
    /// Обычный поток: узел занимает место у соседей.
    #[default]
    Flow,
    /// Абсолютное позиционирование относительно content box родителя: узел не
    /// занимает места в потоке и не влияет на размер родителя.
    Absolute,
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
/// автоматического выбора курсора для узла (см. `light_fw::window`). Имена
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
