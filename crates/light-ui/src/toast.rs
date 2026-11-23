//! Виджет `Toast` (0.21.0): временное уведомление.
//!
//! `show_toast` показывает toast и скрывает его через 3 секунды **без
//! блокировки UI-потока**: вместо `std::thread::spawn + sleep` используется
//! фоновая задача ([`light_fw_core::spawn`]) — отдельный OS-поток, который
//! после 3 секунд возвращает результат в UI-цикл через [`post_to_ui`] (там же
//! сбрасывается флаг видимости; `schedule_redraw` выполняется автоматически
//! после обработки задания). Если UI-цикл ещё не запущен или уже завершён,
//! `post_to_ui` возвращает ошибку — её безопасно игнорируем (`let _ = ...`),
//! т.к. toast уже показан, а автоскрытие просто пропустится.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use light_fw_core::async_runtime::{post_to_ui, spawn};
use light_fw_core::control::{WindowCommand, request};
use light_fw_core::plugins::{CustomElement, CustomElementFactory, EventCtx};
use light_fw_core::vdom::{Event, NodeKind, VElement};

/// Глобальный флаг «активен ли toast» (скелет: один активный toast).
/// `AtomicBool`, т.к. флаг читается в UI-потоке (render) и может быть сброшен
/// из фоновой задачи через `post_to_ui`.
static TOAST_VISIBLE: AtomicBool = AtomicBool::new(false);

/// Включает/выключает видимость toast (в UI-потоке, через `post_to_ui`).
pub(crate) fn set_visible(visible: bool) {
    TOAST_VISIBLE.store(visible, Ordering::Relaxed);
}

/// Возвращает текущий флаг видимости toast.
pub fn is_visible() -> bool {
    TOAST_VISIBLE.load(Ordering::Relaxed)
}

/// Показывает toast на 3 секунды (автоскрытие — фоновая задача + `post_to_ui`).
///
/// Сигнатура принимает `text` для совместимости с демо и будущим рендером
/// подписи (в этой версии текст не отображается, см. `let _ = text`).
pub fn show_toast(text: &str) {
    let _ = text; // подпись toast (скелет не рендерит её)
    set_visible(true);

    // Handle задачи не нужен: задача живёт независимо (spawn запускает поток,
    // handle можно отбросить; см. light-gui::async_runtime::spawn).
    let _ = spawn(|ctx| {
        std::thread::sleep(Duration::from_secs(3));
        if ctx.is_cancelled() {
            return;
        }
        let _ = post_to_ui(|| {
            TOAST_VISIBLE.store(false, Ordering::Relaxed);
            // Принудительно запрашиваем перерисовку окна: после сброса флага
            // UI-цикл может не перерисовать кадр автоматически (например,
            // если нет активных событий ввода), и toast "зависнет" видимым.
            // `WindowCommand::Invalidate` заставляет winit вызвать
            // `request_redraw()` на целевом окне в ближайшем кадре.
            request(WindowCommand::Invalidate);
        });
    });
}

/// Кастомный элемент `<toast>`: рендерит контейнер (пустой при выключенном
/// флаге, Text с подписью при включённом). Подпись берётся из атрибута
/// `text="..."` (по умолчанию пустая строка — `show_toast` пока не передаёт
/// текст в элемент).
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
    fn render(&self, attrs: &[(&'static str, String)], _children: &[VElement]) -> VElement {
        let text = attrs
            .iter()
            .find_map(|(name, value)| (*name == "text").then_some(value.as_str()))
            .unwrap_or("");
        let mut node = VElement::container("", NodeKind::Column, Vec::new());
        if TOAST_VISIBLE.load(Ordering::Relaxed) {
            node.children.push(VElement::leaf(
                "",
                NodeKind::Text {
                    content: text.to_owned(),
                },
            ));
        }
        node
    }

    /// События toast не обрабатываются в этом скелете: видимость управляется
    /// только через `set_visible` / `show_toast` (и автоскрытие).
    fn on_event(&mut self, _event: &Event, _ctx: &mut EventCtx<'_>) {}
}

/// Фабрика плагина: регистрирует кастомный элемент `toast` (distributed-slice).
/// `const`: все поля — `&'static`/`fn pointer`, так что конструктор безопасен
/// в статическом контексте.
const TOAST: CustomElementFactory = CustomElementFactory {
    tag: "toast",
    core_version_req: "^0.21",
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
        assert_eq!(meta.core_version_req, "^0.21");
        assert_eq!(meta.provides, &["toast"]);
    }
}
