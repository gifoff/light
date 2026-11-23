//! Виджет `Tooltip` (0.21.0): кастомный элемент `<tooltip>`.
//!
//! Показывает текст из атрибута `text` при наведении курсора, прячет при
//! уходе. Состояние живёт в самом элементе (`std::cell::Cell<bool>`) и
//! мутируется через [`CustomElement::on_event`] при `Mouseenter`/`Mouseleave`;
//! перерисовка запрашивается через [`EventCtx::schedule_redraw`].
//!
//! Регистрация — через `linkme::distributed_slice(REGISTERED_COMPONENTS)`
//! (как в `examples/plugin-demo`), поэтому подключение крейта-зависимости
//! автоматически делает тег `<tooltip>` доступным в runtime-шаблонах.

use std::cell::Cell;

use light_fw_core::plugins::{CustomElement, CustomElementFactory, EventCtx};
use light_fw_core::vdom::{Event, NodeKind, VElement};

/// Кастомный элемент: строит поддерево (Column + Text) с текстом из
/// атрибута `text`, показывает его только пока курсор над элементом
/// (`is_hovered == true`); при `false` — пустой Column без детей.
pub struct Tooltip {
    is_hovered: Cell<bool>,
}

impl Default for Tooltip {
    fn default() -> Self {
        Self {
            is_hovered: Cell::new(false),
        }
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
        let text = attrs
            .iter()
            .find(|(name, _)| *name == "text")
            .map(|(_, value)| value.as_str())
            .unwrap_or("")
            .to_owned();

        // Пока курсор не над элементом — пустой контейнер (без визуального
        // содержимого). При `is_hovered` добавляем Text с подписью.
        let mut node = VElement::container("", NodeKind::Column, Vec::new());
        if self.is_hovered.get() {
            node.children
                .push(VElement::leaf("", NodeKind::Text { content: text }));
        }
        node
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx<'_>) {
        match event {
            Event::Mouseenter => {
                self.is_hovered.set(true);
                ctx.schedule_redraw();
            }
            Event::Mouseleave => {
                self.is_hovered.set(false);
                ctx.schedule_redraw();
            }
            _ => {}
        }
    }
}

const TOOLTIP: CustomElementFactory = CustomElementFactory {
    tag: "tooltip",
    core_version_req: "^0.21",
    crate_name: "light-ui",
    provides: &["tooltip"],
    factory: || Box::new(Tooltip::default()),
};

#[linkme::distributed_slice(light_fw_core::plugins::REGISTERED_COMPONENTS)]
static TOOLTIP_ENTRY: CustomElementFactory = TOOLTIP;

#[cfg(test)]
mod tests {
    use super::*;
    use light_fw_core::plugins::downcast_custom;

    /// `render()` до hover — пустой Column (без текстовых детей).
    #[test]
    fn tooltip_initial_render_is_empty() {
        let tt = Tooltip::default();
        let v = tt.render(&[("text", "Hi".into())], &[]);
        assert_eq!(v.children.len(), 0, "до hover детей нет");
    }

    /// `render()` после `Mouseenter` — непустой Column с текстом из `text`.
    #[test]
    fn tooltip_shows_text_after_mouseenter() {
        let mut tt = Tooltip::default();
        // Пустой замыкание для schedule_redraw — в тесте не важен.
        let noop = || {};
        let mut ctx = EventCtx::new(&noop);
        tt.on_event(&Event::Mouseenter, &mut ctx);
        let v = tt.render(&[("text", "Hi".into())], &[]);
        assert_eq!(v.children.len(), 1, "после Mouseenter один дочерний Text");
        match &v.children[0].kind {
            NodeKind::Text { content } => assert_eq!(content, "Hi"),
            other => panic!("ожидался Text, получил {:?}", other),
        }
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
    }

    /// Плагин регистрируется под тегом `tooltip` (distributed-slice lookup).
    #[test]
    fn tooltip_registered_in_distributed_slice() {
        let factory = light_fw_core::plugins::lookup("tooltip").expect("фабрика tooltip найдена");
        let elem: Box<dyn CustomElement> = (factory.factory)();
        let tt: &Tooltip = downcast_custom(elem.as_ref()).expect("даункаст к Tooltip");
        assert_eq!(tt.tag(), "tooltip");
    }
}
