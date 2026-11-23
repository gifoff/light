//! Виджет `Toast` (0.21.0): временное уведомление.
//!
//! `show_toast` / `show_toast_cfg` показывают toast и скрывают его через
//! заданное время **без блокировки UI-потока**: вместо `std::thread::spawn +
//! sleep` используется фоновая задача ([`light_fw_core::spawn`]) — отдельный
//! OS-поток, который после таймаута возвращает результат в UI-цикл через
//! [`post_to_ui`] (там же сбрасывается флаг видимости;
//! `WindowCommand::Invalidate` заставляет winit перерисовать кадр). Если
//! UI-цикл ещё не запущен или уже завершён, `post_to_ui` возвращает ошибку —
//! её безопасно игнорируем (`let _ = ...`).
//!
//! ## Повторные клики (0.21.0, генерация)
//!
//! Каждый вызов `show_toast*` увеличивает счётчик-генерацию
//! [`TOAST_GEN`]. Задача автоскрытия запоминает генерацию на момент запуска;
//! если к моменту пробуждения генерация изменилась (новый клик перезапустил
//! таймер), старый hide-колбэк просто пропускается: флаг видимости трогается
//! только «свежей» задачей. Без этого тост исчезал в случайный момент — через
//! несколько кликов первый таймер срабатывал раньше последнего показа.
//!
//! ## Атрибуты элемента `<toast>`
//!
//! - `text="..."` — подпись (по умолчанию пусто);
//! - `duration="3000"` — длительность в мс, хранится как data-метка
//!   (информативно; программная длительность задаётся в
//!   [`show_toast_cfg`] / [`ToastConfig`]);
//! - `position="top-right|top-left|bottom-right|bottom-left"` — позиция.
//!   Элемент позиционируется абсолютными атрибутами контейнера (`left`/
//!   `top`/`right`), поэтому здесь позиция хранится как data-метка; layout
//!   берёт явные координаты из атрибутов узла.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use light_fw_core::async_runtime::{post_to_ui, spawn};
use light_fw_core::control::{WindowCommand, request};
use light_fw_core::plugins::{CustomElement, CustomElementFactory, EventCtx};
use light_fw_core::vdom::{Event, NodeKind, VElement};
use light_fw_core::{Edges, Position};

/// Глобальный флаг «активен ли toast» (скелет: один активный toast).
/// `AtomicBool`, т.к. флаг читается в UI-потоке (render) и может быть сброшен
/// из фоновой задачи через `post_to_ui`.
static TOAST_VISIBLE: AtomicBool = AtomicBool::new(false);

/// Счётчик поколений toast: увеличивается при каждом `show_toast*`.
/// Задача автоскрытия запоминает своё поколение и пропускает сброс, если
/// к моменту пробуждения показан более свежий toast.
static TOAST_GEN: AtomicU64 = AtomicU64::new(0);

/// Включает/выключает видимость toast (в UI-потоке, через `post_to_ui`).
pub(crate) fn set_visible(visible: bool) {
    TOAST_VISIBLE.store(visible, Ordering::Relaxed);
}

/// Возвращает текущий флаг видимости toast.
pub fn is_visible() -> bool {
    TOAST_VISIBLE.load(Ordering::Relaxed)
}

/// Настройка toast'а: длительность в миллисекундах и позиция.
#[derive(Debug, Clone, Copy, Default)]
pub struct ToastConfig {
    /// Длительность показа, мс (0 — без автоскрытия). По умолчанию 3000.
    pub duration_ms: u64,
    /// Позиция: `top-right` (дефолт), `top-left`, `bottom-right`,
    /// `bottom-left`. Читается элемент `<toast>` при рендере.
    pub position: &'static str,
}

impl ToastConfig {
    const DEFAULT_DURATION_MS: u64 = 3000;
    const DEFAULT_POSITION: &'static str = "top-right";

    /// Конфигурат с дефолтными значениями (3 с, `top-right`).
    pub fn new() -> Self {
        Self {
            duration_ms: Self::DEFAULT_DURATION_MS,
            position: Self::DEFAULT_POSITION,
        }
    }

    /// Устанавливает длительность в мс.
    pub fn with_duration_ms(mut self, ms: u64) -> Self {
        self.duration_ms = ms;
        self
    }

    /// Устанавливает позицию.
    pub fn with_position(mut self, position: &'static str) -> Self {
        self.position = position;
        self
    }
}

/// Показывает toast на 3 секунды с дефолтной позицией `top-right`
/// (автоскрытие — фоновая задача + `post_to_ui`).
///
/// Повторный вызов перезапускает таймер (генерация [`TOAST_GEN`]).
pub fn show_toast(text: &str) {
    let cfg = ToastConfig::new();
    show_toast_cfg(text, &cfg);
}

/// Показывает toast с явной конфигурацией (длительность + позиция).
///
/// Повторный вызов перезапускает таймер: старая задача автоскрытия видит
/// смещённое поколение [`TOAST_GEN`] и пропускает сброс, поэтому «короткий
/// после длинного» исчезает в правильный момент, а длинный — не гасит новый.
pub fn show_toast_cfg(text: &str, cfg: &ToastConfig) {
    let _ = text; // подпись (скелет: один активный toast, текст берёт элемент
    // из атрибута `text` или остаётся пустым)
    // Новое поколение: предыдущие hide-задачи станут stale.
    let this_gen = TOAST_GEN.fetch_add(1, Ordering::AcqRel) + 1;
    set_visible(true);
    // Критично (ТЗ 0.21.0): сразу после показа нужно принудить UI-цикл
    // перерисовать кадр — иначе vdom не перерендерится и toast визуально
    // не появится, хотя флаг видимости уже true. `post_to_ui` гарантирует,
    // что invalidate уходит в UI-поток; если цикл ещё не запущен — ошибка
    // безопасно игнорируется (тот же паттерн, что и в hide-ветке).
    let _ = post_to_ui(|| request(WindowCommand::Invalidate));
    if cfg.duration_ms == 0 {
        return; // без автоскрытия
    }
    // Копируем значение до запуска closure'а (closure не может заимствовать
    // `cfg`: borrow не переживает переход в фоновый поток).
    let ms = cfg.duration_ms;
    let _ = spawn(move |ctx| {
        std::thread::sleep(Duration::from_millis(ms));
        if ctx.is_cancelled() {
            return;
        }
        // Поколение сменилось (повторный клик) — не прячём чужой toast.
        let gen_now = TOAST_GEN.load(Ordering::Acquire);
        if gen_now != this_gen {
            return;
        }
        let _ = post_to_ui(|| {
            TOAST_VISIBLE.store(false, Ordering::Relaxed);
            request(WindowCommand::Invalidate);
        });
    });
}

/// Кастомный элемент `<toast>`: рендерит контейнер (пустой при выключенном
/// флаге, Text с подписью при включённом). Подпись — из атрибута `text`;
/// `duration`/`position` сохраняются в data-атрибутах (информативно;
/// программная длительность задаётся в `show_toast_cfg`).
pub struct Toast;

impl Default for Toast {
    fn default() -> Self {
        Self
    }
}

impl CustomElement for Toast {
    fn tag(&self) -> &'static str {
        "toast"
    }

    /// Рендер: пустой Column, если флаг выключен; Column с Text — если включён.
    ///
    /// Позиция (`top-right`/`top-left`/`bottom-right`/`bottom-left`) применяется
    /// абсолютным позиционированием относительно content box родителя: узел
    /// не занимает места в потоке и «прилипает» к углу окна. Отступ по краю —
    /// 16 логических пикселей.
    fn render(&self, attrs: &[(&'static str, String)], _children: &[VElement]) -> VElement {
        let mut node = VElement::container("", NodeKind::Column, Vec::new());
        // Data-метки: сохраняем длительность и позицию в data-атрибутах
        // дерева (информативно; программная длительность задаётся в
        // `show_toast_cfg`). Ключи — строковые литералы (`&'static str`).
        for (name, value) in attrs {
            let key = match *name {
                "duration" => "data-duration",
                "position" => "data-position",
                _ => continue,
            };
            node.attrs.push((key, value.clone()));
        }
        if TOAST_VISIBLE.load(Ordering::Relaxed) {
            let text = attrs
                .iter()
                .find_map(|(name, value)| (*name == "text").then_some(value.as_str()))
                .unwrap_or("");
            node.children.push(VElement::leaf(
                "",
                NodeKind::Text {
                    content: text.to_owned(),
                },
            ));
            // Абсолютное позиционирование по позиции (см. doc).
            // `padding` внутри колонки даёт внутренний отступ текста от края.
            let pad = 16.0_f32;
            node.style.position = Position::Absolute;
            node.style.padding = Edges::all(pad);
            node.style.font_size = Some(14.0);
            node.style.background = Some(0xFF20242B); // тёмная плашка
            node.style.font_color = Some(0xFFFFFFFF); // белый текст
            // Задаём смещения в зависимости от угла. Родительский content box
            // здесь — корневой контейнер окна (600×400 в демо), поэтому
            // «правый верхний угол» = abs_top + abs_right = 16, остальные 0.
            // (Координаты берутся относительно правого/нижнего края родителя.)
            let position = attrs
                .iter()
                .find(|(name, _)| *name == "position")
                .map(|(_, v)| v.as_str())
                .unwrap_or("top-right");
            match position {
                "top-left" => {
                    node.style.abs_top = pad;
                    node.style.abs_left = pad;
                }
                "bottom-left" => {
                    node.style.abs_bottom = pad;
                    node.style.abs_left = pad;
                }
                "bottom-right" => {
                    node.style.abs_bottom = pad;
                    node.style.abs_right = pad;
                }
                // Дефолт: top-right
                _ => {
                    node.style.abs_top = pad;
                    node.style.abs_right = pad;
                }
            }
        }
        node
    }

    /// События toast не обрабатываются: видимость управляется только через
    /// `set_visible` / `show_toast*` (и автоскрытие).
    fn on_event(&mut self, _event: &Event, _ctx: &mut EventCtx<'_>) {}
}

/// Фабрика плагина: регистрирует кастомный элемент `toast` (distributed-slice).
/// `const`: все поля — `&'static`/`fn pointer`, так что конструктор безопасен
/// в статическом контексте.
const TOAST: CustomElementFactory = CustomElementFactory {
    tag: "toast",
    core_version_req: "^0.20",
    crate_name: "light-ui",
    provides: &["toast"],
    factory: || Box::new(Toast),
};

#[linkme::distributed_slice(light_fw_core::plugins::REGISTERED_COMPONENTS)]
static TOAST_ENTRY: CustomElementFactory = TOAST;

#[cfg(test)]
mod tests {
    use super::*;

    /// Изначально toast невидим.
    #[test]
    fn toast_initially_hidden() {
        TOAST_VISIBLE.store(false, Ordering::Relaxed);
        assert!(!is_visible());
    }

    /// `set_visible(true)` — единственный синхронный путь (UI-поток). Тест
    /// фиксирует контракт флага: видимость управляется этим setter'ом.
    /// Фоновая автоскрытийная задача проверяется только на отсутствии
    /// блокировок (`show_toast` возвращает немедленно), а не по её действию —
    /// это требует синхронизации с UI-циклом, что выходит за рамки unit-теста.
    #[test]
    fn set_visible_toggle_works() {
        TOAST_VISIBLE.store(false, Ordering::Relaxed);
        assert!(!is_visible());
        set_visible(true);
        assert!(is_visible(), "set_visible(true) делает toast видимым");
        set_visible(false); // cleanup
    }

    /// `ToastConfig::new` даёт дефолты: 3000 мс и `top-right`.
    #[test]
    fn config_defaults() {
        let cfg = ToastConfig::new();
        assert_eq!(cfg.duration_ms, 3000);
        assert_eq!(cfg.position, "top-right");
    }

    /// Builder-сеттеры `with_duration_ms` / `with_position` работают.
    #[test]
    fn config_builder() {
        let cfg = ToastConfig::new()
            .with_duration_ms(5000)
            .with_position("bottom-left");
        assert_eq!(cfg.duration_ms, 5000);
        assert_eq!(cfg.position, "bottom-left");
    }

    /// `show_toast_cfg` с `duration_ms = 0` показывает toast и НЕ ставит
    /// автоскрытие (флаг остаётся true; caller отвечает за скрытие).
    #[test]
    fn show_with_zero_duration_keeps_visible() {
        TOAST_VISIBLE.store(false, Ordering::Relaxed);
        let cfg = ToastConfig::new().with_duration_ms(0);
        show_toast_cfg("x", &cfg);
        assert!(is_visible(), "duration=0: toast должен остаться видимым");
        // Cleanup: сбрасываем флаг вручную.
        set_visible(false);
    }

    /// Элемент `<toast>` рендерит пустой контейнер, пока флаг не включён.
    #[test]
    fn toast_render_empty_when_hidden() {
        TOAST_VISIBLE.store(false, Ordering::Relaxed);
        let t = Toast;
        let v = t.render(&[("text", "Hello".into())], &[]);
        assert!(
            v.children.is_empty(),
            "пустой toast не должен содержать Text-дети"
        );
    }

    /// При включённом флаге `<toast>` содержит Text с подписью из атрибутов.
    #[test]
    fn toast_render_shows_text_when_visible() {
        TOAST_VISIBLE.store(true, Ordering::Relaxed);
        let t = Toast;
        let v = t.render(&[("text", "Hi".into())], &[]);
        assert_eq!(v.children.len(), 1, "включённый toast — 1 дочерний Text");
        TOAST_VISIBLE.store(false, Ordering::Relaxed); // cleanup
    }

    /// Атрибуты `duration`/`position` сохраняются в data-метках дерева.
    #[test]
    fn toast_render_keeps_meta_attrs() {
        TOAST_VISIBLE.store(false, Ordering::Relaxed);
        let t = Toast;
        let v = t.render(
            &[("duration", "5000".into()), ("position", "top-left".into())],
            &[],
        );
        // Ключи — строковые литералы (`&'static str`), поэтому `*k` даёт &str.
        let keys: Vec<&str> = v.attrs.iter().map(|(k, _)| *k).collect();
        assert!(keys.contains(&"data-duration"), "data-duration должен быть");
        assert!(keys.contains(&"data-position"), "data-position должен быть");
    }

    /// Плагин регистрируется под тегом `toast` (distributed-slice lookup).
    /// Проверяем метаданные фабрики (тег/crate/core_version) — это контракт
    /// Plugin API; downcast экземпляра покрывается в light-vdom (см. там).
    #[test]
    fn toast_registered_in_distributed_slice() {
        let entry = light_fw_core::plugins::lookup("toast");
        assert!(entry.is_some(), "тег `toast` должен быть зарегистрирован");
        let meta = entry.expect("только проверили Some");
        assert_eq!(meta.tag, "toast");
        assert_eq!(meta.crate_name, "light-ui");
        assert_eq!(meta.core_version_req, "^0.20");
        assert_eq!(meta.provides, &["toast"]);
    }
}
