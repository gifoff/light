use super::events::{
    Align, Edges, Offset, Overflow, Position, RequiredIndicator, Size, WhiteSpace,
};

#[derive(Debug, Clone, Copy, PartialEq)]
/// Полная структура стилей элемента VDOM.
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
    /// (`flex-shrink`/`flex`). По умолчанию `1`, как в CSS; `0` — не сжиматься.
    pub flex_shrink: f32,
    /// Участие в нормальном потоке (`position`).
    pub position: Position,
    /// Переполнение по горизонтали (`overflow-x`).
    pub overflow_x: Overflow,
    /// Переполнение по вертикали (`overflow-y`).
    pub overflow_y: Overflow,
    /// Режим переноса текста (`white-space`). `None` наследует значение
    /// родителя; у корня действует [`WhiteSpace::Normal`].
    pub white_space: Option<WhiteSpace>,
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
    /// Смещения абсолютного позиционирования относительно content box родителя
    /// (`top`/`right`/`bottom`/`left`). Действуют только при
    /// [`Position::Absolute`]; в потоке игнорируются. По умолчанию `0`.
    pub abs_top: f32,
    /// Смещение справа (см. [`Self::abs_top`]).
    pub abs_right: f32,
    /// Смещение снизу (см. [`Self::abs_top`]).
    pub abs_bottom: f32,
    /// Смещение слева (см. [`Self::abs_top`]).
    pub abs_left: f32,
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
            flex_shrink: 1.0,
            position: Position::Flow,
            overflow_x: Overflow::Visible,
            overflow_y: Overflow::Visible,
            white_space: None,
            align_x: Align::Start,
            align_y: Align::Start,
            offset_x: Offset::Px(0.0),
            offset_y: Offset::Px(0.0),
            justify_content: Align::Start,
            align_items: Align::Start,
            padding: Edges::ZERO,
            margin: Edges::ZERO,
            abs_top: 0.0,
            abs_right: 0.0,
            abs_bottom: 0.0,
            abs_left: 0.0,
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
