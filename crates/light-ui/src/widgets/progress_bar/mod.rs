//! Виджет **ProgressBar** (0.21.0): кастомный элемент `<progress value="…">`.
//!
//! ## Модель
//!
//! ProgressBar — **потоковый** элемент (`position`/`z` не ставятся): он живёт
//! в нормальном потоке документа и занимает место, как обычный узел. Полоса:
//! внешний контейнер (фон `background`, высота `height`) с внутренним узлом
//! заполнения (цвет `color`), ширина которого равна
//! `clamp(value, 0, max) / max * 100%` (проценты от родителя — [`Size::Percent`]).
//!
//! Атрибуты (все опциональные; дефолты виджета применяются всегда, явный
//! атрибут переопределяет дефолт — та же логика, что у Dialog по `position`):
//!
//! | атрибут        | поле                     | дефолт                 |
//! |----------------|--------------------------|------------------------|
//! | `value`        | доля заполнения         | `0.0`                  |
//! | `max`          | максимум (`value <= max`)| `100.0` (`max<=0 → 100`) |
//! | `color`        | `fill.style.background`  | `#22C55E`              |
//! | `background`   | `root.style.background`  | `#E5E7EB`              |
//! | `height`       | `root.style.height`      | `10` (px)              |
//!
//! Парсинг: цвета — `parse_color` + CSS-имена (как в tooltip/dialog/toast),
//! размеры — собственный `parse_px` (px/число → f32). Неверное
//! числовое значение → `0.0`; `max <= 0` → фолбэк `100.0`.
//!
//! Инвариант (ТЗ 0.21.0): ProgressBar — **в потоке**; `position`/`z` не
//! устанавливаются (в отличие от Dialog/Tooltip).

use light_fw_core::plugins::{CustomElement, CustomElementFactory, EventCtx};
use light_fw_core::vdom::parse::parse_color;
use light_fw_core::vdom::{Event, NodeKind, Size, VElement};

/// Кастомный элемент `<progress>`: полоса заполнения (см. doc модуля).
#[derive(Debug, Default)]
pub struct ProgressBar;

impl CustomElement for ProgressBar {
    fn tag(&self) -> &'static str
    where
        Self: Sized,
    {
        "progress"
    }

    fn render(&self, attrs: &[(&'static str, String)], children: &[VElement]) -> VElement {
        // children намеренно игнорируются: `<progress>` — контейнер без
        // содержимого (по контракту ТЗ 0.21.0); если пользователь вложит
        // текст — он не попадёт в вывод.
        let _ = children;

        // Дефолты виджета (применяются ВСЕГДА; явный атрибут ниже их
        // перекрывает — «механизм» виджета важнее политики по умолчанию):
        let mut value: f32 = 0.0;
        let mut max: f32 = 100.0;
        let mut color: Option<u32> = Some(0x22_C5_5E_FF); // #22C55E (зелёный)
        let mut background: Option<u32> = Some(0xE5_E7_EB_FF); // #E5E7EB (светло-серый)
        let mut height: Option<f32> = Some(10.0); // px

        for (name, val) in attrs {
            match *name {
                "value" => value = parse_f32(val).unwrap_or(0.0),
                "max" => {
                    let m = parse_f32(val).unwrap_or(100.0);
                    // max <= 0 → 100 (инвариант ТЗ): отрицательный/нулевой
                    // максимум бессмыслен, фиксируем 100.
                    if m > 0.0 {
                        max = m;
                    } else {
                        max = 100.0;
                    }
                }
                "color" => color = parse_color_named(val),
                "background" => background = parse_color_named(val),
                "height" => {
                    if let Some(px) = parse_px(val) {
                        height = Some(px);
                    }
                }
                // Остальные атрибуты игнорируются: ProgressBar не принимает
                // `position`/`z`/`padding` (это инвариант: полоса в потоке).
                _ => {}
            }
        }

        // Корень: внешний контейнер (фон + высота).
        let mut root = VElement::container("progress", NodeKind::Column, Vec::new());
        root.style.width = Size::Percent(100.0); // растягиваем на всю ширину родителя
        root.style.background = background;
        if let Some(h) = height {
            root.style.height = Size::Px(h);
        }

        // Узел заполнения: цвет `color`, ширина = clamp(value,0,max)/max*100%.
        // (max всегда > 0 благодаря фолбэку выше; ветка max<=0 — dead code, удалена.)
        let ratio = (value.clamp(0.0, max) / max) * 100.0;
        // Округляем до 3 знаков после запятой: убираем накопление float-шума
        // (например 60.0/200.0*100.0 = 30.000002), сохраняя 0.001% точности —
        // более чем достаточно для ширины полосы в пикселях.
        let rounded = (ratio * 1000.0).round() / 1000.0;
        let fill_width = if rounded.is_finite() { rounded } else { 0.0 };
        let mut fill = VElement::leaf("progress-fill", NodeKind::Column);
        fill.style.width = Size::Percent(fill_width.clamp(0.0, 100.0));
        fill.style.height = Size::Percent(100.0); // заполняем всю высоту родителя
        fill.style.background = color;

        root.children.push(fill);
        root
    }

    /// События ProgressBar не обрабатываются: значение задаётся атрибутом
    /// `value` (или реактивным полем через `{{val}}` в шаблоне).
    fn on_event(&mut self, _event: &Event, _ctx: &mut EventCtx<'_>) {}
}

/// Фабрика плагина: регистрирует кастомный элемент `progress`
/// (distributed-slice). `const`: все поля — `&'static`/`fn pointer`.
///
/// `core_version_req: "^0.21"` — согласовано с tooltip/toast-container/dialog
/// (одна линейка виджетов в light-ui). Кастомный матчер: для 0.x действует
/// minor-floor (`^0.N` совместимо при `cur_minor >= N`, patch игнорируется);
/// pre-release/build-метаданные ядра (например `0.21.0-alpha.1`) игнорируются
/// `parse_triple` — alpha-ядро считается своим release-триплетом. См.
/// `check_core_version` в `light-vdom/src/plugins.rs:201`.
const PROGRESS_BAR: CustomElementFactory = CustomElementFactory {
    tag: "progress",
    core_version_req: "^0.21",
    crate_name: "light-ui",
    provides: &["progress"],
    factory: || Box::new(ProgressBar),
};

#[linkme::distributed_slice(light_fw_core::plugins::REGISTERED_COMPONENTS)]
static PROGRESS_BAR_ENTRY: CustomElementFactory = PROGRESS_BAR;

/// Парсит число в `f32` (пустое → `None`; "42"/"42.5" → `Some`).
///
/// Отдельная от `parse_px` — здесь нет суффикса `px` (числовые атрибуты
/// `value`/`max` чистые числа).
fn parse_f32(value: &str) -> Option<f32> {
    let v = value.trim();
    if v.is_empty() {
        return None;
    }
    v.parse::<f32>().ok()
}

/// Парсит `px`/число в `f32` для высоты (`"10"` или `"10px"` → `10.0`).
fn parse_px(value: &str) -> Option<f32> {
    let v = value.trim();
    if v.is_empty() {
        return None;
    }
    let num = v.strip_suffix("px").unwrap_or(v).trim();
    num.parse::<f32>().ok()
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

#[cfg(test)]
mod tests {
    //! Unit-тесты ProgressBar: дефолты, кастомные цвета, clamp-арифметика
    //! (`value`/`max` → ширина заполнения), парсинг атрибутов и регистрация.

    use super::*;
    use light_fw_core::plugins::lookup;

    /// Рендерит `<progress>` с заданными атрибутами и возвращает корень.
    /// Ключи — `'static` (тот же контракт, что у `CustomElement::render`).
    fn render_with(attrs: &[(&'static str, &str)]) -> VElement {
        let raw: Vec<(&'static str, String)> =
            attrs.iter().map(|(k, v)| (*k, (*v).to_string())).collect();
        ProgressBar.render(&raw, &[])
    }

    /// Берёт внутренний узел заполнения (единственный ребёнок корня).
    fn fill_of(root: &VElement) -> &VElement {
        assert!(root.children.len() == 1, "один fill-узел");
        &root.children[0]
    }

    /// Без атрибутов: `value=0`, `max=100`, `color=#22C55E`,
    /// `background=#E5E7EB`, `height=10px`, `width=100%` (полоса в потоке,
    /// `position=Flow` по умолчанию — НЕ абсолютный оверлей, как Dialog).
    #[test]
    fn default_values() {
        let root = render_with(&[]);
        assert_eq!(root.style.background, Some(0xE5_E7_EB_FF));
        assert_eq!(root.style.width, Size::Percent(100.0));
        assert_eq!(root.style.height, Size::Px(10.0));
        let fill = fill_of(&root);
        assert_eq!(fill.style.background, Some(0x22_C5_5E_FF));
        assert_eq!(fill.style.height, Size::Percent(100.0));
        // value=0 → заполнение 0%.
        assert_eq!(fill.style.width, Size::Percent(0.0));
    }

    /// Явные `color`/`background` переопределяют дефолты (RGB из hex + alpha FF).
    #[test]
    fn custom_color() {
        let root = render_with(&[("color", "#EF4444"), ("background", "#111111")]);
        assert_eq!(root.style.background, Some(0x11_11_11_FF));
        assert_eq!(fill_of(&root).style.background, Some(0xEF_44_44_FF));
    }

    /// CSS-имена цветов тоже работают (делегирование `parse_color_named`).
    #[test]
    fn named_colors() {
        let root = render_with(&[("color", "red"), ("background", "black")]);
        assert_eq!(fill_of(&root).style.background, Some(0xFF_00_00_FF));
        assert_eq!(root.style.background, Some(0x00_00_00_FF));
    }

    /// `value > max` → зажат до `max` (100% заполнения).
    #[test]
    fn value_above_max_clamped() {
        let root = render_with(&[("value", "150"), ("max", "100")]);
        assert_eq!(fill_of(&root).style.width, Size::Percent(100.0));
    }

    /// Отрицательный `value` → зажат до 0 (заполнение 0%).
    #[test]
    fn negative_value_clamped() {
        let root = render_with(&[("value", "-20")]);
        assert_eq!(fill_of(&root).style.width, Size::Percent(0.0));
    }

    /// `max <= 0` → фолбэк на 100 (инвариант ТЗ): `max="0"` не ломает деление.
    #[test]
    fn zero_max_falls_back_to_100() {
        let root = render_with(&[("value", "50"), ("max", "0")]);
        // max=0 → 100; value=50 → 50%.
        assert_eq!(fill_of(&root).style.width, Size::Percent(50.0));
    }

    /// Неверное числовое значение `value` → `0.0` (не падает, не NaN).
    #[test]
    fn invalid_value_parses_to_zero() {
        let root = render_with(&[("value", "abc")]);
        assert_eq!(fill_of(&root).style.width, Size::Percent(0.0));
    }

    /// Арифметика ширины: `value=25` при `max=100` → `25%`; `60/200` → `30%`.
    #[test]
    fn fill_width_math() {
        let a = render_with(&[("value", "25"), ("max", "100")]);
        assert_eq!(fill_of(&a).style.width, Size::Percent(25.0));
        let b = render_with(&[("value", "60"), ("max", "200")]);
        assert_eq!(fill_of(&b).style.width, Size::Percent(30.0));
    }

    /// `height="20"` → `Size::Px(20.0)`; `height="20px"` то же самое.
    #[test]
    fn height_attr_px() {
        let a = render_with(&[("height", "20")]);
        assert_eq!(a.style.height, Size::Px(20.0));
        let b = render_with(&[("height", "20px")]);
        assert_eq!(b.style.height, Size::Px(20.0));
    }

    /// Инвариант: ProgressBar **в потоке** — `position` остаётся `Flow`
    /// (дефолт `Style`), `z` = 0 (дефолт) — в отличие от Dialog (absolute)
    /// и Tooltip (z=1000).
    #[test]
    fn progress_is_flow_not_overlay() {
        let root = render_with(&[]);
        use light_fw_core::vdom::Position;
        assert_eq!(root.style.position, Position::Flow);
        assert_eq!(root.style.z, 0);
    }

    /// Тег зарегистрирован в distributed-slice: `lookup("progress")` находит
    /// фабрику light-ui с `provides == ["progress"]`.
    #[test]
    fn progress_registered_in_distributed_slice() {
        let Some(factory) = lookup("progress") else {
            panic!("фабрика 'progress' не найдена в REGISTERED_COMPONENTS");
        };
        assert_eq!(factory.tag, "progress");
        assert_eq!(factory.crate_name, "light-ui");
        assert_eq!(factory.provides, &["progress"]);
        assert_eq!(factory.core_version_req, "^0.21");
    }
}
