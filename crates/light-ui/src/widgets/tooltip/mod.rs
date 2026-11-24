//! Виджет **Tooltip** (0.21.0, архитектурный рефакторинг).
//!
//! ## Модель
//!
//! Фреймворк — **механизм**, пользователь — **политика**. Tooltip больше
//! **не** управляет своей видимостью и **не** хранит глобальное состояние:
//!
//! * Внутренних ховер-состояний (`Mouseenter`/`Mouseleave`) нет; видимость
//!   полностью под контролем пользователя через `v-if` +
//!   `@mouseenter`/`@mouseleave` в шаблоне (см. ТЗ 0.21.0).
//! * Атрибута `delay` нет: задержка (если нужна) — политика приложения.
//! * Цветов и отступов «по умолчанию» нет: корневой элемент **прозрачен**
//!   по умолчанию, пользователь задаёт в HTML стандартные атрибуты стиля:
//!
//! ```html
//! <tooltip v-if="tip_visible" text="Delayed Tooltip!"
//!          position="absolute" top="40" left="0"
//!          background="black" font-color="white" padding="5" />
//! ```
//!
//! ## Почему `render` применяет стили вручную
//!
//! `CustomElement::render` получает **сырые** атрибуты тега, а runtime
//! просто перекрашивает возвращённый корень в
//! `NodeKind::Custom("tooltip")` (атрибуты стиля к тегу **не** применяются
//! автоматически — см. `light_vdom::runtime::parse::build_custom_element`).
//! Поэтому `render` сам вытаскивает `background`/`font-color`/`position`/
//! `top`/`left`/`padding`/`font-size` и ставит их в style возвращённого
//! корня, используя парсеры VDOM (`light_vdom::vdom::parse::*`), чтобы
//! поведение совпадало со статическими элементами.
//!
//! Регистрация — через `linkme::distributed_slice(REGISTERED_COMPONENTS)`,
//! поэтому подключение `light-ui` как зависимости автоматически делает тег
//! `<tooltip>` доступным в runtime-шаблонах.

use std::str::FromStr;

use light_fw_core::plugins::{CustomElement, CustomElementFactory, EventCtx};
use light_fw_core::vdom::parse::{parse_color, parse_position, parse_size_or_auto};
use light_fw_core::vdom::{Edges, Event, NodeKind, Style, VElement};

/// Кастомный элемент `<tooltip>`: показывает подпись рядом с курсором /
/// элементом, когда пользователь решает это сделать через `v-if`.
///
/// Инстанс не хранит состояния — `render` чистая функция от атрибутов.
#[derive(Debug, Default)]
pub struct Tooltip;

/// Разбирает числовое значение пиксельной координаты (`"40"`, `"40px"`).
/// Пустое значение → `0.0`; нечисло → `None`.
fn parse_px(value: &str) -> Option<f32> {
    let value = value.trim();
    if value.is_empty() {
        return Some(0.0);
    }
    let number = value.strip_suffix("px").unwrap_or(value).trim();
    f32::from_str(number).ok()
}

/// Разбирает цвет: CSS-имена (через lower-case) + общие VDOM-форматы
/// `#RRGGBB[AA]`/`0x…` (порядок байт — **`0xRRGGBBAA`**, т.е. alpha в
/// младшем байте; совпадает с VDOM-тестами: `#FF0000` → `0xFF0000FF`).
/// light-ui поддерживает **наименования** для удобного HTML-авторинга,
/// которые runtime-шаблоны тоже понимают: `black`, `white`, `red`, `green`,
/// `blue`, `yellow`, `orange`, `purple`, `gray`/`grey`. Всё остальное
/// делегируется общему VDOM-парсеру [`parse_color`].
fn parse_color_named(value: &str) -> Option<u32> {
    match value.trim().to_ascii_lowercase().as_str() {
        "black" => Some(0x00_00_00_FF),
        "white" => Some(0xFF_FF_FF_FF),
        "red" => Some(0xFF_00_00_FF),
        "green" => Some(0x00_FF_00_FF),
        "blue" => Some(0x00_00_FF_FF),
        "yellow" => Some(0xFF_FF_00_FF),
        "orange" => Some(0xFF_7F_00_FF),
        "purple" => Some(0x80_00_80_FF),
        "gray" | "grey" => Some(0x80_80_80_FF),
        _ => parse_color(value),
    }
}

/// Применяет стандартные атрибуты стиля к корню tooltip'а.
///
/// Обрабатываемые атрибуты (все опциональные, по умолчанию — «прозрачный,
/// в потоке, без отступов»):
///
/// | атрибут       | поле стиля            | дефолт         |
/// |---------------|-----------------------|----------------|
/// | `background`  | `Style::background`   | `None`         |
/// | `font-color`  | `Style::font_color`   | `None`         |
/// | `position`    | `Style::position`     | `Position::Flow` |
/// | `top`         | `Style::abs_top`      | `0.0`          |
/// | `left`        | `Style::abs_left`     | `0.0`          |
/// | `right`       | `Style::abs_right`    | `0.0`          |
/// | `bottom`      | `Style::abs_bottom`   | `0.0`          |
/// | `padding`     | `Style::padding`      | `Edges::ZERO`  |
/// | `width`       | `Style::width`        | `Size::Auto`   |
/// | `height`      | `Style::height`       | `Size::Auto`   |
/// | `font-size`   | `Style::font_size`    | `None`         |
fn apply_style_attrs(style: &mut Style, attrs: &[(&'static str, String)]) {
    for (name, value) in attrs {
        match *name {
            "background" => {
                style.background = parse_color_named(value);
            }
            "font-color" => {
                style.font_color = parse_color_named(value);
            }
            "position" => {
                style.position = parse_position(value);
            }
            "top" => {
                if let Some(px) = parse_px(value) {
                    style.abs_top = px;
                }
            }
            "left" => {
                if let Some(px) = parse_px(value) {
                    style.abs_left = px;
                }
            }
            "right" => {
                if let Some(px) = parse_px(value) {
                    style.abs_right = px;
                }
            }
            "bottom" => {
                if let Some(px) = parse_px(value) {
                    style.abs_bottom = px;
                }
            }
            "padding" => {
                if let Some(px) = parse_px(value) {
                    style.padding = Edges::all(px);
                }
            }
            "width" => {
                style.width = parse_size_or_auto(value);
            }
            "height" => {
                style.height = parse_size_or_auto(value);
            }
            "font-size" => {
                if let Some(px) = parse_px(value) {
                    style.font_size = Some(px);
                }
            }
            // Остальные атрибуты (`text`, `v-*`-директивы уже отсеяны,
            // пользовательские `data-*`) игнорируются: tooltip прозрачен
            // ко всему, что не относится к его стилю.
            _ => {}
        }
    }
}

impl CustomElement for Tooltip {
    /// Имя тега (для `downcast_custom` и диагностики).
    fn tag(&self) -> &'static str {
        "tooltip"
    }

    /// Рендерит tooltip: корневой `Column` с подданным стилем (см.
    /// [`apply_style_attrs`]) и один дочерний `Text`-лист со значением
    /// атрибута `text`.
    ///
    /// Корневой тег остаётся `Column` (runtime перекрасит его в
    /// `NodeKind::Custom("tooltip")`). Если `text` не задан, дочерний
    /// лист пуст — tooltip отрисует прозрачный, но занимающий место узел
    /// (чтобы позиция `v-if="false"` не «прыгала»).
    fn render(&self, attrs: &[(&'static str, String)], _children: &[VElement]) -> VElement {
        let text = attrs
            .iter()
            .find_map(|(name, value)| (*name == "text").then_some(value.clone()))
            .unwrap_or_default();

        let mut root = VElement::container("tooltip", NodeKind::Column, Vec::new());
        apply_style_attrs(&mut root.style, attrs);
        // Тултип не перехватывает мышь (стандарт CSS `pointer-events: none`):
        // иначе, оказавшись под курсором, он вытеснял бы мышь с родителя
        // (`Mouseleave`) и исчезал сам. Hit-тест пропускает узлы с
        // `pointer_events == Some(false)`, поэтому тултип и его дети
        // «прозрачны» для указателя. Приоритет — над любым пользовательским
        // атрибутом `pointer-events` тега (тултип по определению неинтерактивен).
        root.style.pointer_events = Some(false);

        root.children.push(VElement::leaf(
            "tooltip-text",
            NodeKind::Text { content: text },
        ));
        root
    }

    /// События tooltip не обрабатываются: видимость управляется только
    /// через `v-if` (пользовательский `@mouseenter`/`@mouseleave` в шаблоне).
    fn on_event(&mut self, _event: &Event, _ctx: &mut EventCtx<'_>) {}
}

/// Фабрика плагина: регистрирует кастомный элемент `tooltip`
/// (distributed-slice). `const`: все поля — `&'static`/`fn pointer`.
const TOOLTIP: CustomElementFactory = CustomElementFactory {
    tag: "tooltip",
    core_version_req: "^0.20",
    crate_name: "light-ui",
    provides: &["tooltip"],
    factory: || Box::new(Tooltip),
};

#[linkme::distributed_slice(light_fw_core::plugins::REGISTERED_COMPONENTS)]
static TOOLTIP_ENTRY: CustomElementFactory = TOOLTIP;

#[cfg(test)]
mod tests {
    //! Unit-тесты: контракт tooltip'а (атрибуты → style) и регистрация.

    use super::*;
    use light_fw_core::plugins::lookup;
    use light_fw_core::vdom::Position;

    /// `background="black"` → `Style::background == Some(0x000000FF)`
    /// (порядок байт — RRGGBBAA, alpha в младшем байте; совпадает с
    /// VDOM-тестами: `#FF0000` → `0xFF0000FF`).
    #[test]
    fn tooltip_applies_background_attr() {
        let mut el = VElement::container("t", NodeKind::Column, vec![]);
        let attrs = vec![("background", "black".to_string())];
        apply_style_attrs(&mut el.style, &attrs);
        assert_eq!(el.style.background, Some(0x00_00_00_FF));
    }

    /// `font-color="white"` → `Style::font_color == Some(0xFFFFFFFF)`.
    #[test]
    fn tooltip_applies_font_color_attr() {
        let mut el = VElement::container("t", NodeKind::Column, vec![]);
        let attrs = vec![("font-color", "white".to_string())];
        apply_style_attrs(&mut el.style, &attrs);
        assert_eq!(el.style.font_color, Some(0xFFFFFFFF));
    }

    /// `position="absolute"` + `top="40"` + `left="0"` → абсолютная
    /// позиция с точными координатами (демо из ТЗ).
    #[test]
    fn tooltip_applies_position_and_offsets() {
        let mut el = VElement::container("t", NodeKind::Column, vec![]);
        let attrs = vec![
            ("position", "absolute".to_string()),
            ("top", "40".to_string()),
            ("left", "0".to_string()),
        ];
        apply_style_attrs(&mut el.style, &attrs);
        assert_eq!(el.style.position, Position::Absolute);
        assert_eq!(el.style.abs_top, 40.0);
        assert_eq!(el.style.abs_left, 0.0);
        assert_eq!(el.style.abs_right, 0.0);
        assert_eq!(el.style.abs_bottom, 0.0);
    }

    /// `padding="5"` → все четыре стороны `Edges::all(5.0)`.
    #[test]
    fn tooltip_applies_padding_attr() {
        let mut el = VElement::container("t", NodeKind::Column, vec![]);
        let attrs = vec![("padding", "5".to_string())];
        apply_style_attrs(&mut el.style, &attrs);
        assert_eq!(el.style.padding, Edges::all(5.0));
    }

    /// Без атрибутов стиль по умолчанию: `Position::Flow`, цвета `None`,
    /// padding нулевой (tooltip прозрачен и в потоке).
    #[test]
    fn tooltip_default_style_is_flow_and_transparent() {
        let mut el = VElement::container("t", NodeKind::Column, vec![]);
        apply_style_attrs(&mut el.style, &[]);
        assert_eq!(el.style.position, Position::Flow);
        assert!(el.style.background.is_none());
        assert!(el.style.font_color.is_none());
        assert!(el.style.padding.is_zero());
    }

    /// `render` с полным набором атрибутов из ТЗ: корень — Column (будет
    /// перекрашен в `NodeKind::Custom("tooltip")` runtime'ом), один дочерний
    /// Text с содержимым `text="Delayed Tooltip!"`.
    #[test]
    fn tooltip_render_produces_text_child() {
        let t = Tooltip;
        let attrs = vec![
            ("text", "Delayed Tooltip!".to_string()),
            ("position", "absolute".to_string()),
            ("top", "40".to_string()),
            ("left", "0".to_string()),
            ("background", "black".to_string()),
            ("font-color", "white".to_string()),
            ("padding", "5".to_string()),
        ];
        let rendered = t.render(&attrs, &[]);
        assert!(matches!(rendered.kind, NodeKind::Column));
        assert_eq!(rendered.style.background, Some(0x00_00_00_FF));
        assert_eq!(rendered.style.font_color, Some(0xFF_FF_FF_FF));
        assert_eq!(rendered.style.padding, Edges::all(5.0));
        // Тултип не перехватывает мышь — иначе он вытеснял бы курсор с
        // родителя и исчезал сам (см. pointer-events в hit-тесте).
        assert_eq!(rendered.style.pointer_events, Some(false));
        assert_eq!(rendered.children.len(), 1);
        match &rendered.children[0].kind {
            NodeKind::Text { content } => assert_eq!(content, "Delayed Tooltip!"),
            other => panic!("expected Text child, got {other:?}"),
        }
    }

    /// Даже если пользователь явно передаст `pointer-events="auto"`,
    /// tooltip остаётся неинтерактивным: приоритет у внутреннего
    /// `Some(false)` (виджет по определению не «под курсором»).
    #[test]
    fn tooltip_render_forces_pointer_events_off_despite_attr() {
        let t = Tooltip;
        let attrs = vec![("pointer-events", "auto".to_string())];
        let rendered = t.render(&attrs, &[]);
        assert_eq!(rendered.style.pointer_events, Some(false));
    }

    /// `render` без `text` → дочерний Text пустой (tooltip прозрачен,
    /// но занимает место — позиция `v-if="false"` стабильна).
    #[test]
    fn tooltip_render_without_text_is_empty() {
        let t = Tooltip;
        let rendered = t.render(&[], &[]);
        assert_eq!(rendered.children.len(), 1);
        match &rendered.children[0].kind {
            NodeKind::Text { content } => assert!(content.is_empty()),
            other => panic!("expected Text child, got {other:?}"),
        }
    }

    /// Регистрация в distributed-slice: `lookup("tooltip")` возвращает
    /// фабрику light-ui (тег доступен в runtime-шаблонах без магии).
    #[test]
    fn tooltip_registered_in_distributed_slice() {
        let factory = lookup("tooltip");
        assert!(factory.is_some(), "tag \"tooltip\" must be registered");
        let factory = factory.expect("checked");
        assert_eq!(factory.crate_name, "light-ui");
        assert_eq!(factory.provides, &["tooltip"]);
    }
}
