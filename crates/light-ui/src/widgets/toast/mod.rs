//! Виджет **Toast** (0.21.0, архитектурный рефакторинг).
//!
//! ## Модель
//!
//! Фреймворк — **механизм**, пользователь — **политика**. Toast больше **не**
//! хардкодит цвет фона / цвет текста и **не** «изобретает» позицию
//! (`top-right` и т.п.):
//!
//! * [`show_toast(text, duration_ms)`] добавляет toast в менеджер
//!   [`ToastManager`] (`thread_local`, т.к. `CustomElement` — не `Send+Sync`)
//!   и запускает фоновую задачу, которая через `duration_ms` удаляет toast
//!   (по `id`) и принудительно перерисовывает UI ([`WindowCommand::Invalidate`]).
//! * `<toast-container>` рендерит **все** активные toast'ы вертикальным
//!   столбиком (`margin_bottom: 10` у каждого — эквивалент `gap="10"`;
//!   `gap` — неатрибут раскладки в light, см. ТЗ 0.21.0).
//! * Контейнер **прозрачен** по умолчанию. Пользователь задаёт в HTML:
//!   `<toast-container position="absolute" right="10" top="10" gap="10" />`
//!   и при желании `background`/`font-color`/`font-size`/`padding`.
//!
//! `CustomElement::render` получает **сырые** атрибуты тега, поэтому
//! `render` сам вытаскивает `position`/`top`/`right`/`bottom`/`left`/
//! `background`/`font-color`/`font-size`/`padding` и ставит их в style
//! возвращённого корня (атрибуты стиля **не** применяются к тегу
//! автоматически — runtime просто перекрашивает корень в
//! `NodeKind::Custom("toast-container")`). Парсеры совпадают с VDOM:
//! `position`/`width`/`height` — через `light_vdom::vdom::parse::*`,
//! цвета — через общий парсер (формат `0xAARRGGBB`, имён нет — только
//! `#RRGGBB[AA]`/`0x…`; «black»-типы задаются как `#000000`).
//!
//! регистрация: подключение `light-ui` как зависимости автоматически делает
//! тег `<toast-container>` доступным в runtime-шаблонах.

use std::cell::RefCell;
use std::str::FromStr;
use std::time::{Duration, Instant};

use light_fw_core::async_runtime::{post_to_ui, spawn};
use light_fw_core::control::{WindowCommand, request};
use light_fw_core::plugins::{CustomElement, CustomElementFactory, EventCtx};
use light_fw_core::vdom::parse::{parse_color, parse_position, parse_size_or_auto};
use light_fw_core::vdom::{Edges, Event, NodeKind, Style, VElement};

/// Один активный toast.
#[derive(Debug, Clone)]
pub struct ActiveToast {
    /// Уникальный идентификатор (счётчик [`ToastManager::next_id`]).
    pub id: u64,
    /// Подпись, показанная пользователю.
    pub text: String,
    /// Момент создания (для отладки / потенциального TTL-лога).
    pub created_at: Instant,
    /// Длительность жизни в мс (0 — toast живёт вечно, автоскрытия нет).
    pub duration_ms: u64,
}

/// Менеджер активных toast'ов.
///
/// Состояние живёт в `thread_local!` (т.к. `CustomElement` — не
/// `Send + Sync`, а `render` вызывается в UI-потоке). Фоновая задача
/// автоскрытия **не** удерживает заимствование `RefCell` на время сна —
/// она спит, а затем мутирует менеджер через `post_to_ui` (в UI-потоке).
///
/// Инициализация константна (`{ toasts: Vec::new(), next_id: 0 }`) —
/// это позволяет использовать `thread_local! { static X = const { … } }`.
pub struct ToastManager {
    /// Активные toast'ы в порядке добавления (столбик сверху вниз).
    pub toasts: Vec<ActiveToast>,
    /// Счётчик `id` (монотонно растёт; не пересобирается при удалении).
    pub next_id: u64,
}

impl Default for ToastManager {
    fn default() -> Self {
        Self {
            toasts: Vec::new(),
            next_id: 0,
        }
    }
}

impl ToastManager {
    /// Добавляет toast и возвращает его `id`.
    pub fn push(&mut self, text: String, duration_ms: u64) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.toasts.push(ActiveToast {
            id,
            text,
            created_at: Instant::now(),
            duration_ms,
        });
        id
    }

    /// Удаляет toast по `id` (возвращает `true`, если был).
    pub fn remove_by_id(&mut self, id: u64) -> bool {
        let before = self.toasts.len();
        self.toasts.retain(|t| t.id != id);
        self.toasts.len() < before
    }
}

thread_local! {
    /// Тестовая фиксированная инициализация (константный блок).
    static TOAST_MANAGER: RefCell<ToastManager> = const {
        RefCell::new(ToastManager { toasts: Vec::new(), next_id: 0 })
    };
}

/// Показывает toast: добавляет в [`ToastManager`] и запускает фоновую
/// задачу автоскрытия через `duration_ms` (если > 0).
///
/// Вызывать **только из UI-потока** (обработчик события). Функция
/// непаникующая: если цикл событий ещё не запущен, `post_to_ui`/`spawn`
/// просто теряются (тот же паттерн, что и в `hide`-ветке).
pub fn show_toast(text: &str, duration_ms: u64) {
    let id = TOAST_MANAGER.with(|m| m.borrow_mut().push(text.to_owned(), duration_ms));
    // Немедленная перерисовка, чтобы vdom перерендерился и toast появился.
    let _ = post_to_ui(|| request(WindowCommand::Invalidate));
    if duration_ms == 0 {
        return; // вечноживущий toast (без автоскрытия).
    }
    // Фоновая задача: спать `duration_ms`, затем через `post_to_ui` удалить
    // **конкретный** toast по `id` и принудить invalidate. Не держим
    // заимствование RefCell через сна — mutation только в UI-потоке.
    let _ = spawn(move |ctx| {
        std::thread::sleep(Duration::from_millis(duration_ms));
        if ctx.is_cancelled() {
            return;
        }
        let _ = post_to_ui(move || {
            TOAST_MANAGER.with(|m| {
                let removed = m.borrow_mut().remove_by_id(id);
                // Invalidate нужно даже если removal вернул `false` (например,
                // toast уже удалён) — нет, не нужно. Оптимизация: invalidate
                // только если реально что-то поменялось.
                if removed {
                    request(WindowCommand::Invalidate);
                }
            });
        });
    });
}

/// Кастомный элемент `<toast-container>`: вертикальный столбик **всех**
/// активных toast'ов. Прозрачен по умолчанию; атрибуты стиля тега
/// (`position`/`top`/`right`/`bottom`/`left`/`background`/`font-color`/
/// `font-size`/`padding`) переносит `render` в style возвращённого корня.
///
/// Каждый toast — `VElement::leaf(NodeKind::Text)` с `margin_bottom: 10`
/// (эквивалент `gap="10"`; `gap` — неатрибут раскладки в light).
pub struct ToastContainer;

impl Default for ToastContainer {
    fn default() -> Self {
        Self
    }
}

/// Парсит `px`/число в `f32` (`10` или `"10px"` → `10.0`; пустое → `None`).
fn parse_px(value: &str) -> Option<f32> {
    let v = value.trim();
    if v.is_empty() {
        return None;
    }
    let num = v.strip_suffix("px").unwrap_or(v).trim();
    f32::from_str(num).ok()
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

/// Применяет стандартные атрибуты стиля тега к корню контейнера.
///
/// Обрабатываемые атрибуты (все опциональные; по умолчанию — «прозрачный,
/// в потоке, без отступов»):
///
/// | атрибут       | поле стиля         | дефолт             |
/// |---------------|--------------------|--------------------|
/// | `background`  | `Style::background`| `None`             |
/// | `font-color`  | `Style::font_color`| `None`             |
/// | `position`    | `Style::position`  | `Position::Flow`   |
/// | `top`/`right`/`bottom`/`left` | `Style::abs_*` | `0.0` |
/// | `padding`     | `Style::padding`   | `Edges::ZERO`      |
/// | `width`       | `Style::width`     | `Size::Auto`       |
/// | `height`      | `Style::height`    | `Size::Auto`       |
/// | `font-size`   | `Style::font_size` | `None`             |
///
/// Парсинг цвета: [parse_color_named] — CSS-имена (`black`, `white`, …)
/// + общие VDOM-форматы `#RRGGBB[AA]`/`0x…` (порядок байт — `0xAARRGGBB`).
fn apply_container_style(style: &mut Style, attrs: &[(&'static str, String)]) {
    for (name, value) in attrs {
        match *name {
            "background" => style.background = parse_color_named(value),
            "font-color" => style.font_color = parse_color_named(value),
            "position" => style.position = parse_position(value),
            "top" => {
                if let Some(px) = parse_px(value) {
                    style.abs_top = px;
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
            "left" => {
                if let Some(px) = parse_px(value) {
                    style.abs_left = px;
                }
            }
            "padding" => {
                if let Some(px) = parse_px(value) {
                    style.padding = Edges::all(px);
                }
            }
            "width" => style.width = parse_size_or_auto(value),
            "height" => style.height = parse_size_or_auto(value),
            "font-size" => {
                if let Some(px) = parse_px(value) {
                    style.font_size = Some(px);
                }
            }
            // Остальные атрибуты (`gap`, `data-*`, …) игнорируются: `gap`
            // не является атрибутом раскладки в light — расстояние между
            // toast'ами задаётся `margin_bottom` каждого дочернего узла.
            _ => {}
        }
    }
}

impl CustomElement for ToastContainer {
    fn tag(&self) -> &'static str {
        "toast-container"
    }

    /// Рендер: Column из всех активных toast'ов.
    ///
    /// Атрибуты стиля тега переносим в style корня **сами** (см.
    /// [`apply_container_style`]). Каждый toast — Text-leaf с
    /// `margin_bottom: 10` (эквивалент `gap="10"`; `gap` — неатрибут
    /// раскладки в light, поэтому ТЗ-`gap="10"` интерпретируется как
    /// 10px-отступ между toast'ами).
    fn render(&self, attrs: &[(&'static str, String)], _children: &[VElement]) -> VElement {
        let mut root = VElement::container("", NodeKind::Column, Vec::new());
        apply_container_style(&mut root.style, attrs);

        // --- Активные toast'ы ---
        let toasts = TOAST_MANAGER.with(|m| m.borrow().toasts.clone());
        for t in toasts {
            let mut leaf = VElement::leaf(
                "toast",
                NodeKind::Text {
                    content: t.text.clone(),
                },
            );
            leaf.style.margin = Edges {
                top: 0.0,
                right: 0.0,
                bottom: 10.0,
                left: 0.0,
            };
            root.children.push(leaf);
        }

        root
    }

    /// События toast-контейнера не обрабатываются: видимость управляется
    /// только через `show_toast` / автоскрытие.
    fn on_event(&mut self, _event: &Event, _ctx: &mut EventCtx<'_>) {}
}

/// Фабрика плагина: регистрирует кастомный элемент `toast-container`
/// (distributed-slice). `const`: все поля — `&'static`/`fn pointer`.
const TOAST_CONTAINER: CustomElementFactory = CustomElementFactory {
    tag: "toast-container",
    core_version_req: "^0.20",
    crate_name: "light-ui",
    provides: &["toast-container"],
    factory: || Box::new(ToastContainer),
};

#[linkme::distributed_slice(light_fw_core::plugins::REGISTERED_COMPONENTS)]
static TOAST_CONTAINER_ENTRY: CustomElementFactory = TOAST_CONTAINER;

#[cfg(test)]
mod tests {
    //! Unit-тесты: `show_toast`/автоскрытие, контракт стиля контейнера и
    //! регистрация в distributed-slice.

    use super::*;
    use light_fw_core::plugins::lookup;
    use light_fw_core::vdom::Position;

    /// `show_toast` добавляет запись в thread-local менеджер (сразу после
    /// возврата). ID монотонно растёт; `duration_ms` сохраняется.
    #[test]
    fn show_toast_pushes_into_manager() {
        show_toast("hello", 100);
        let toasts = TOAST_MANAGER.with(|m| m.borrow().toasts.clone());
        assert_eq!(toasts.len(), 1);
        assert_eq!(toasts[0].text, "hello");
        assert_eq!(toasts[0].duration_ms, 100);
    }

    /// Менеджер: `push` возвращает следующий id, `remove_by_id` удаляет
    /// конкретный (не первый) и возвращает `true`; повторное удаление —
    /// `false`.
    #[test]
    fn manager_push_and_remove_by_id() {
        let mut m = ToastManager::default();
        let a = m.push("a".into(), 1);
        let b = m.push("b".into(), 2);
        assert_eq!(a, 0);
        assert_eq!(b, 1);
        assert!(m.remove_by_id(1));
        assert!(!m.remove_by_id(1)); // уже удалён
        assert!(!m.remove_by_id(99)); // такого не было
        assert_eq!(m.toasts.len(), 1);
        assert_eq!(m.toasts[0].id, 0);
    }

    /// Контейнер: `position="absolute" right="10" top="10"` → абсолютная
    /// позиция с точными смещениями (демо из ТЗ: «топ-право, отступ 10»).
    #[test]
    fn container_applies_position_and_offsets() {
        let mut el = VElement::container("t", NodeKind::Column, vec![]);
        let attrs = vec![
            ("position", "absolute".to_string()),
            ("right", "10".to_string()),
            ("top", "10".to_string()),
        ];
        apply_container_style(&mut el.style, &attrs);
        assert_eq!(el.style.position, Position::Absolute);
        assert_eq!(el.style.abs_top, 10.0);
        assert_eq!(el.style.abs_right, 10.0);
        assert_eq!(el.style.abs_left, 0.0);
        assert_eq!(el.style.abs_bottom, 0.0);
    }

    /// `background="black"` + `font-color="white"` → CSS-имена
    /// интерпретируются в RRGGBBAA-порядке light: `0x000000FF` / `0xFFFFFFFF`.
    #[test]
    fn container_applies_named_colors() {
        let mut el = VElement::container("t", NodeKind::Column, vec![]);
        let attrs = vec![
            ("background", "black".to_string()),
            ("font-color", "white".to_string()),
        ];
        apply_container_style(&mut el.style, &attrs);
        assert_eq!(el.style.background, Some(0x00_00_00_FF));
        assert_eq!(el.style.font_color, Some(0xFF_FF_FF_FF));
    }

    /// `#RRGGBB`-формат тоже поддерживается (через общий VDOM-парсер):
    /// `#FF7F00` (оранжевый) → `0xFF7F00FF` (RRGGBBAA).
    #[test]
    fn container_applies_hex_color() {
        let mut el = VElement::container("t", NodeKind::Column, vec![]);
        let attrs = vec![("background", "#FF7F00".to_string())];
        apply_container_style(&mut el.style, &attrs);
        assert_eq!(el.style.background, Some(0xFF_7F_00_FF));
    }

    /// `padding="5"` → все четыре стороны `Edges::all(5.0)`; `gap="10"`
    /// **не** является атрибутом раскладки (интерпретируется как
    /// `margin_bottom` каждого дочернего toast'а — см. doc-комментарий).
    #[test]
    fn container_applies_padding_and_ignores_gap() {
        let mut el = VElement::container("t", NodeKind::Column, vec![]);
        let attrs = vec![("padding", "5".to_string()), ("gap", "10".to_string())];
        apply_container_style(&mut el.style, &attrs);
        assert_eq!(el.style.padding, Edges::all(5.0));
        // gap не меняет стиль корня (нет поля `gap` в Style).
    }

    /// Без атрибутов контейнер прозрачен: цвета `None`, позиция в потоке,
    /// padding нулевой (полностью «прозрачный» контейнер по умолчанию).
    #[test]
    fn container_default_style_is_flow_and_transparent() {
        let mut el = VElement::container("t", NodeKind::Column, vec![]);
        apply_container_style(&mut el.style, &[]);
        assert_eq!(el.style.position, Position::Flow);
        assert!(el.style.background.is_none());
        assert!(el.style.font_color.is_none());
        assert!(el.style.padding.is_zero());
    }

    /// Регистрация в distributed-slice: `lookup("toast-container")`
    /// возвращает фабрику light-ui (тег доступен в runtime-шаблонах).
    #[test]
    fn toast_container_registered_in_distributed_slice() {
        let factory = lookup("toast-container");
        assert!(factory.is_some(), "tag \"toast-container\" must be registered");
        let factory = factory.expect("checked");
        assert_eq!(factory.crate_name, "light-ui");
        assert_eq!(factory.provides, &["toast-container"]);
    }
}
