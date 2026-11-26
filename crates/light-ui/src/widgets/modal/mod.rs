//! Виджет **Modal** (0.21.0): модальное окно с заголовком и контентом.
//!
//! ## Модель
//!
//! Фреймворк — **механизм**, пользователь — **политика**:
//!
//! * `<modal title="…">контент</modal>` рендерит корневой `Column` с
//!   белым фоном, рамкой 2px (`#888888`), padding 16, `z = 5000`
//!   (модальное окно — поверх остальных элементов, включая tooltip'ы c
//!   их дефолтом `z = 1000`) и **`position = absolute` по умолчанию**
//!   (отступы `left`/`top` = 50px, чтобы окно не прилипало к краю).
//!   Modal — оверлей по определению: без явного `position` оно **не**
//!   в потоке и не сдвигает контент. Явный атрибут `position="flow"`
//!   переопределяет дефолт.
//! * Атрибут `title` добавляет дочерний `Text`-лист; остальные атрибуты
//!   (стиль) к корню **не** применяются автоматически (runtime просто
//!   перекрашивает его в `NodeKind::Custom("modal")`) — как и для
//!   [`tooltip`]/[`toast-container`], виджет задаёт свой стиль явно.
//! * Закрытие окна — политика приложения: пользователь управляется
//!   видимостью через `v-if`, как и у `tooltip`.

use std::str::FromStr;

use light_fw_core::plugins::{CustomElement, CustomElementFactory, EventCtx};
use light_fw_core::vdom::parse::{parse_color, parse_position, parse_size_or_auto};
use light_fw_core::vdom::{Edges, Event, NodeKind, Position, Style, VElement};

/// Кастомный элемент `<modal>`: модальное окно с заголовком (`title`)
/// и произвольным контентом (дети шаблона). Инстанс состояния не хранит —
/// `render` чистая функция от атрибутов.
#[derive(Debug, Default)]
pub struct Modal;

/// Разбирает числовое значение пиксельной координаты (`"40"`, `"40px"`).
/// Пустое значение → `None`; нечисло → `None`.
fn parse_px(value: &str) -> Option<f32> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    let number = value.strip_suffix("px").unwrap_or(value).trim();
    f32::from_str(number).ok()
}

/// Разбирает цвет: CSS-имена (через lower-case) + общие VDOM-форматы
/// `#RRGGBB[AA]`/`0x…` (порядок байт — **`0xRRGGBBAA`**, т.е. alpha в
/// младшем байте; совпадает с VDOM-тестами: `#FF0000` → `0xFF0000FF`).
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

/// Применяет стандартные атрибуты стиля к корню modal'а.
///
/// Обрабатываемые атрибуты (все опциональные; по умолчанию — «белый фон,
/// рамка 2px, padding 16, z=5000»):
///
/// | атрибут       | поле стиля            | дефолт              |
/// |---------------|-----------------------|---------------------|
/// | `background`  | `Style::background`   | `Some(0xFFFFFFFF)`  |
/// | `font-color`  | `Style::font_color`   | `None`              |
/// | `position`    | `Style::position`     | `Position::Absolute`|
/// | `top`         | `Style::abs_top`      | `50.0` (без position) |
/// | `left`        | `Style::abs_left`     | `50.0` (без position) |
/// | `right`       | `Style::abs_right`    | `0.0`               |
/// | `bottom`      | `Style::abs_bottom`   | `0.0`               |
/// | `padding`     | `Style::padding`      | `Edges::all(16.0)`  |
/// | `width`       | `Style::width`        | `Size::Auto`        |
/// | `height`      | `Style::height`       | `Size::Auto`        |
/// | `font-size`   | `Style::font_size`    | `None`              |
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
            // Остальные атрибуты игнорируются: modal сам задаёт рамку и z.
            _ => {}
        }
    }
}

impl CustomElement for Modal {
    /// Имя тега (для `downcast_custom` и диагностики).
    fn tag(&self) -> &'static str {
        "modal"
    }

    /// Рендерит modal: корневой `Column` с подданным стилем (см.
    /// [`apply_style_attrs`]), заголовком из атрибута `title` и всеми
    /// дочерними узлами шаблона.
    ///
    /// Корневой тег остаётся `Column` (runtime перекрасит его в
    /// `NodeKind::Custom("modal")`). Значения по умолчанию (рамка 2px
    /// `#888888`, белый фон, padding 16, `z = 5000`) **вкладываются
    /// всегда** — как и `pointer_events = false` у tooltip, виджетный
    /// дефолт важнее политики приложения: модальное окно по определению
    /// верхний слой поверх прочих элементов.
    ///
    /// **Позиция по умолчанию:** без явного атрибута `position` корень
    /// получает `Position::Absolute` со смещениями `abs_left`/`abs_top = 50px`
    /// (окно — оверлей, не в потоке; отступ, чтобы не прилипало к краю).
    /// Явный `position="flow"` переопределяет дефолт (пользовательская
    /// политика важнее, когда явно задана).
    fn render(&self, attrs: &[(&'static str, String)], children: &[VElement]) -> VElement {
        let title = attrs
            .iter()
            .find_map(|(name, value)| (*name == "title").then_some(value.clone()))
            .unwrap_or_default();

        let mut root = VElement::container("modal", NodeKind::Column, Vec::new());
        // Дефолты модального окна: верхний слой (5000 > tooltip'ов 1000).
        root.style.z = 5000;
        root.style.background = Some(0xFF_FF_FF_FF);
        root.style.border_size = 2.0;
        root.style.border_color = Some(0xFF_88_88_88);
        root.style.padding = Edges::all(16.0);

        // Без явного `position` — absolute-оверлей с отступом 50px от края.
        if !attrs.iter().any(|(name, _)| *name == "position") {
            root.style.position = Position::Absolute;
            root.style.abs_left = 50.0;
            root.style.abs_top = 50.0;
        }

        apply_style_attrs(&mut root.style, attrs);

        if !title.is_empty() {
            let title_node = VElement::leaf("modal-title", NodeKind::Text { content: title });
            root.children.push(title_node);
        }

        for child in children {
            root.children.push(child.clone());
        }

        root
    }

    /// События modal не обрабатываются: видимость управляется только
    /// через `v-if` (пользовательский `@click` на кнопке закрытия в шаблоне).
    fn on_event(&mut self, _event: &Event, _ctx: &mut EventCtx<'_>) {}
}

/// Фабрика плагина: регистрирует кастомный элемент `modal`
/// (distributed-slice). `const`: все поля — `&'static`/`fn pointer`.
const MODAL: CustomElementFactory = CustomElementFactory {
    tag: "modal",
    core_version_req: "^0.21",
    crate_name: "light-ui",
    provides: &["modal"],
    factory: || Box::new(Modal),
};

#[linkme::distributed_slice(light_fw_core::plugins::REGISTERED_COMPONENTS)]
static MODAL_ENTRY: CustomElementFactory = MODAL;

#[cfg(test)]
mod tests {
    //! Unit-тесты: контракт modal'а (атрибуты → style) и регистрация.

    use super::*;
    use light_fw_core::plugins::lookup;

    /// Без атрибутов корень получает дефолты: белый фон, рамка 2px
    /// `#888888` (→ `0xFF888888`), padding 16, `z = 5000` (верхний слой).
    #[test]
    fn modal_default_style_is_modal() {
        let m = Modal;
        let rendered = m.render(&[], &[]);
        assert_eq!(rendered.style.background, Some(0xFF_FF_FF_FF));
        assert_eq!(rendered.style.border_size, 2.0);
        assert_eq!(rendered.style.border_color, Some(0xFF_88_88_88));
        assert_eq!(rendered.style.padding, Edges::all(16.0));
        assert_eq!(rendered.style.z, 5000);
    }

    /// Без атрибута `position` корень — **оверлей**: `Position::Absolute`,
    /// `z = 5000`, отступ 50px от верхнего левого угла (окно не прилипает
    /// к краю и не сдвигает контент в потоке).
    #[test]
    fn modal_defaults_to_absolute_with_z_5000() {
        let m = Modal;
        let rendered = m.render(&[], &[]);
        assert_eq!(rendered.style.position, Position::Absolute);
        assert_eq!(rendered.style.z, 5000);
        assert_eq!(rendered.style.abs_top, 50.0);
        assert_eq!(rendered.style.abs_left, 50.0);
    }

    /// Явный `position="flow"` **переопределяет** дефолт absolute:
    /// пользовательская политика важнее виджетного дефолта, когда
    /// задана явно. При этом `z = 5000` сохраняется (виджетный слой —
    /// всегда верхний).
    #[test]
    fn modal_explicit_position_flow_overrides_default() {
        let m = Modal;
        let attrs = vec![("position", "flow".to_string())];
        let rendered = m.render(&attrs, &[]);
        assert_eq!(rendered.style.position, Position::Flow);
        assert_eq!(rendered.style.z, 5000);
    }

    /// `title="Hello"` → один дочерний Text с этим содержимым.
    #[test]
    fn modal_render_produces_title_child() {
        let m = Modal;
        let attrs = vec![("title", "Hello".to_string())];
        let rendered = m.render(&attrs, &[]);
        assert_eq!(rendered.children.len(), 1);
        match &rendered.children[0].kind {
            NodeKind::Text { content } => assert_eq!(content, "Hello"),
            other => panic!("expected Text child, got {other:?}"),
        }
    }

    /// Дочерние узлы шаблона сохраняются после заголовка.
    #[test]
    fn modal_render_keeps_template_children() {
        let m = Modal;
        let attrs = vec![("title", "T".to_string())];
        let children = vec![VElement::leaf(
            "body",
            NodeKind::Text {
                content: "body".into(),
            },
        )];
        let rendered = m.render(&attrs, &children);
        assert_eq!(rendered.children.len(), 2);
        match (&rendered.children[0].kind, &rendered.children[1].kind) {
            (NodeKind::Text { content: t }, NodeKind::Text { content: b }) => {
                assert_eq!(t, "T");
                assert_eq!(b, "body");
            }
            other => panic!("unexpected children: {other:?}"),
        }
    }

    /// Без `title` заголовка нет — только дети шаблона.
    #[test]
    fn modal_render_without_title_is_empty_title() {
        let m = Modal;
        let rendered = m.render(&[], &[]);
        assert!(rendered.children.is_empty());
    }

    /// `background="black"` переопределяет дефолтный белый фон:
    /// CSS-имена интерпретируются в RRGGBBAA-порядке light.
    #[test]
    fn modal_applies_background_attr() {
        let mut el = VElement::container("m", NodeKind::Column, vec![]);
        el.style.background = Some(0xFF_FF_FF_FF);
        let attrs = vec![("background", "black".to_string())];
        apply_style_attrs(&mut el.style, &attrs);
        assert_eq!(el.style.background, Some(0x00_00_00_FF));
    }

    /// `padding="8"` → все четыре стороны `Edges::all(8.0)`.
    #[test]
    fn modal_applies_padding_attr() {
        let mut el = VElement::container("m", NodeKind::Column, vec![]);
        el.style.padding = Edges::all(16.0);
        let attrs = vec![("padding", "8".to_string())];
        apply_style_attrs(&mut el.style, &attrs);
        assert_eq!(el.style.padding, Edges::all(8.0));
    }

    /// Регистрация в distributed-slice: `lookup("modal")` возвращает
    /// фабрику light-ui (тег доступен в runtime-шаблонах без магии).
    #[test]
    fn modal_registered_in_distributed_slice() {
        let factory = lookup("modal");
        assert!(factory.is_some(), "tag \"modal\" must be registered");
        let factory = factory.expect("checked");
        assert_eq!(factory.crate_name, "light-ui");
        assert_eq!(factory.provides, &["modal"]);
    }
}
