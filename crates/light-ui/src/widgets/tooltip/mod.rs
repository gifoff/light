//! Виджет `Tooltip` (0.21.0): кастомный элемент `<tooltip>`.
//!
//! Показывает текст из атрибута `text` при наведении курсора, прячет при
//! уходе. Состояние живёт в самом элементе (`Cell<bool>` / `AtomicU64`) и
//! мутируется через [`CustomElement::on_event`] при `Mouseenter`/`Mouseleave`;
//! перерисовка запрашивается через [`EventCtx::schedule_redraw`] (в UI-потоке)
//! или [`WindowCommand::Invalidate`] (из фоновой задачи через `post_to_ui`).
//!
//! ## Задержка показа (атрибут `delay`, 0.21.0)
//!
//! Атрибут `delay="500"` (мс, по умолчанию `0`) означает: после `Mouseenter`
//! tooltip **не показывается сразу**, а ждёт заданное время; если за это время
//! пришёл `Mouseleave` — показ отменяется. Реализация:
//!
//! - `Mouseenter` увеличивает генерацию [`TOOLTIP_GEN`], ставит
//!   `is_hovered=false` и запускает фоновую задачу ([`light_fw_core::spawn`]):
//!   она засыпает на `delay_ms`, затем через [`post_to_ui`] проверяет, что
//!   генерация не сменилась и мышь ещё внутри (по `exited_at > entered_at`
//!   — отменено), и тогда включает видимость + `Invalidate`.
//! - `Mouseleave` ставит `is_hovered=false`, обновляет `exited_at` и
//!   увеличиваем генерацию (так задача, проснувшаяся позже, видит свежий
//!   `exited_at` и пропускает показ).
//!
//! При `delay=0` поведение совпадает с прежним (немедленный показ), но теперь
//! через `post_to_ui`.
//!
//! Регистрация — через `linkme::distributed_slice(REGISTERED_COMPONENTS)`
//! (как в `examples/plugin-demo`), поэтому подключение крейта-зависимости
//! автоматически делает тег `<tooltip>` доступным в runtime-шаблонах.

use std::cell::Cell;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use light_fw_core::async_runtime::{post_to_ui, spawn};
use light_fw_core::control::{WindowCommand, request};
use light_fw_core::plugins::{CustomElement, CustomElementFactory, EventCtx};
use light_fw_core::vdom::{Event, NodeKind, Size, VElement};

/// Глобальная «состояние hover» tooltip'а (скелет: один активный tooltip).
///
/// Используется фоновой задачей таймера (через `post_to_ui`), чтобы решить
/// race: мышь ушла, пока таймер спал. UI-поток (render) тоже читает этот
/// флаг; сброс при Mouseleave — синхронно в UI-потоке.
static TOOLTIP_VISIBLE: AtomicBool = AtomicBool::new(false);

/// Момент последнего Mouseenter (epoch millis). 0 — не в hover'е.
static TOOLTIP_ENTERED_AT: AtomicU64 = AtomicU64::new(0);

/// Момент последнего Mouseleave (epoch millis). 0 — выход не был.
static TOOLTIP_EXITED_AT: AtomicU64 = AtomicU64::new(0);

/// Кастомный элемент: строит поддерево (Column + Text) с текстом из
/// атрибута `text`, показывает его только пока курсор над элементом
/// (`TOOLTIP_VISIBLE == true`); при `false` — пустой Column без детей.
pub struct Tooltip {
    /// Кэшированная задержка (мс) — обновляется на каждом `render` из
    /// атрибута `delay` (дефолт 0). Чтится в `on_event` (UI-поток).
    delay_ms: Cell<u64>,
}

impl Default for Tooltip {
    fn default() -> Self {
        Self {
            delay_ms: Cell::new(0),
        }
    }
}

impl Tooltip {
    /// Читает атрибут `delay` (мс) из текущего набора атрибутов.
    fn read_delay(attrs: &[(&'static str, String)]) -> u64 {
        attrs
            .iter()
            .find(|(name, _)| *name == "delay")
            .and_then(|(_, v)| v.parse().ok())
            .unwrap_or(0)
    }
}

impl CustomElement for Tooltip {
    fn tag(&self) -> &'static str
    where
        Self: Sized,
    {
        "tooltip"
    }

    fn render(&self, attrs: &[(&'static str, String)], _children: &[VElement]) -> VElement {
        // Кэшируем `delay` (обновляем, если атрибут изменился между рендерами).
        self.delay_ms.set(Self::read_delay(attrs));
        let text = attrs
            .iter()
            .find(|(name, _)| *name == "text")
            .map(|(_, value)| value.as_str())
            .unwrap_or("")
            .to_owned();

        // Пока курсор не над элементом — пустой контейнер (без визуального
        // содержимого). При видимости добавляем Text с подписью.
        let mut node = VElement::container("", NodeKind::Column, Vec::new());
        if TOOLTIP_VISIBLE.load(Ordering::Relaxed) {
            node.children
                .push(VElement::leaf("", NodeKind::Text { content: text }));
        }
        // Гарантируем минимальный размер 50×20, чтобы зона наведения была
        // видимой и кликабельной даже без содержимого (синтетические события
        // Mouseenter срабатывают только при ненулевом rect).
        node.style.min_width = Some(Size::Px(50.0));
        node.style.min_height = Some(Size::Px(20.0));
        node
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx<'_>) {
        match event {
            Event::Mouseenter => {
                let now_ms = now_epoch_millis();
                TOOLTIP_ENTERED_AT.store(now_ms, Ordering::Relaxed);
                TOOLTIP_EXITED_AT.store(0, Ordering::Relaxed);
                TOOLTIP_VISIBLE.store(false, Ordering::Relaxed);
                let delay_ms = self.delay_ms.get();
                if delay_ms == 0 {
                    // Немедленный показ (сохраняет старый контракт: нет
                    // задержки → show сейчас).
                    TOOLTIP_VISIBLE.store(true, Ordering::Relaxed);
                    ctx.schedule_redraw();
                } else {
                    // Фоновая задача: после delay_ms через `post_to_ui`
                    // проверяет, что мышь ещё внутри, и тогда включает
                    // видимость + `Invalidate`. Если за это время пришёл
                    // Mouseleave — `exited_at > entered_at` и показ отменён.
                    let _ = spawn(move |c| {
                        std::thread::sleep(Duration::from_millis(delay_ms));
                        if c.is_cancelled() {
                            return;
                        }
                        let _ = post_to_ui(|| {
                            let entered = TOOLTIP_ENTERED_AT.load(Ordering::Acquire);
                            let exited = TOOLTIP_EXITED_AT.load(Ordering::Acquire);
                            if entered == 0 {
                                // Мышь уже вышла (или не входила).
                                return;
                            }
                            if exited > entered {
                                // Мышь ушла до пробуждения — показ отменён.
                                return;
                            }
                            TOOLTIP_VISIBLE.store(true, Ordering::Release);
                            // Принудительная перерисовка: без активных событий
                            // ввода UI-цикл может не перерисовать кадр сам.
                            request(WindowCommand::Invalidate);
                        });
                    });
                }
            }
            Event::Mouseleave => {
                TOOLTIP_VISIBLE.store(false, Ordering::Relaxed);
                TOOLTIP_EXITED_AT.store(now_epoch_millis(), Ordering::Relaxed);
                ctx.schedule_redraw();
            }
            _ => {}
        }
    }
}

/// Текущий epoch в миллисекундах (монотонный для сравнения enter/exit).
fn now_epoch_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

const TOOLTIP: CustomElementFactory = CustomElementFactory {
    tag: "tooltip",
    core_version_req: "^0.20",
    crate_name: "light-ui",
    provides: &["tooltip"],
    factory: || Box::new(Tooltip::default()),
};

#[linkme::distributed_slice(light_fw_core::plugins::REGISTERED_COMPONENTS)]
static TOOLTIP_ENTRY: CustomElementFactory = TOOLTIP;

#[cfg(test)]
mod tests {
    use super::*;

    /// `render()` до hover — пустой Column (без текстовых детей).
    #[test]
    fn tooltip_initial_render_is_empty() {
        TOOLTIP_VISIBLE.store(false, Ordering::Relaxed);
        let tt = Tooltip::default();
        let v = tt.render(&[("text", "Hi".into())], &[]);
        assert_eq!(v.children.len(), 0, "до hover детей нет");
    }

    /// `render()` после `Mouseenter` — непустой Column с текстом из `text`.
    /// Без атрибута `delay` (дефолт 0) — немедленный показ, как раньше.
    #[test]
    fn tooltip_shows_text_after_mouseenter() {
        let mut tt = Tooltip::default();
        let noop = || {};
        let mut ctx = EventCtx::new(&noop);
        tt.on_event(&Event::Mouseenter, &mut ctx);
        let v = tt.render(&[("text", "Hi".into())], &[]);
        assert_eq!(v.children.len(), 1, "после Mouseenter один дочерний Text");
        match &v.children[0].kind {
            NodeKind::Text { content } => assert_eq!(content, "Hi"),
            other => panic!("ожидался Text, получил {:?}", other),
        }
        TOOLTIP_VISIBLE.store(false, Ordering::Relaxed); // cleanup
    }

    /// `Mouseleave` — прячет текст обратно.
    #[test]
    fn tooltip_hides_text_after_mouseleave() {
        let mut tt = Tooltip::default();
        let noop = || {};
        let mut ctx = EventCtx::new(&noop);
        tt.on_event(&Event::Mouseenter, &mut ctx);
        tt.on_event(&Event::Mouseleave, &mut ctx);
        let v = tt.render(&[("text", "Hi".into())], &[]);
        assert_eq!(v.children.len(), 0, "после Mouseleave детей нет");
        TOOLTIP_VISIBLE.store(false, Ordering::Relaxed); // cleanup
    }

    /// `delay` attribute is read and cached by `render`.
    #[test]
    fn tooltip_read_delay_attr() {
        let tt = Tooltip::default();
        let _ = tt.render(&[("delay", "500".into()), ("text", "x".into())], &[]);
        assert_eq!(tt.delay_ms.get(), 500, "delay=500 кэшируется");
    }

    /// Плагин регистрируется под тегом `tooltip` (distributed-slice lookup).
    #[test]
    fn tooltip_registered_in_distributed_slice() {
        let factory = light_fw_core::plugins::lookup("tooltip").expect("фабрика tooltip найдена");
        let elem: Box<dyn CustomElement> = (factory.factory)();
        let tt: &Tooltip =
            light_fw_core::plugins::downcast_custom(elem.as_ref()).expect("даункаст к Tooltip");
        assert_eq!(tt.tag(), "tooltip");
    }
}
