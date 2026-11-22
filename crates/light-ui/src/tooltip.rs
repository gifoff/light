//! Виджет `Tooltip` (скелет, 0.21.0): кастомный элемент `<tooltip>`.
//!
//! Показывает текст из атрибута `text` при наведении. Регистрация — через
//! `linkme::distributed_slice(REGISTERED_COMPONENTS)` (как в
//! `examples/plugin-demo`), поэтому подключение крейта-зависимости автоматически
//! делает тег `<tooltip>` доступным в runtime-шаблонах.

use light_fw_core::plugins::{CustomElement, CustomElementFactory};
use light_fw_core::vdom::{NodeKind, VElement};

/// Кастомный элемент-заглушка: строит простое поддерево (Column + Text)
/// с текстом из атрибута `text`. Поведение «показать при наведении» не
/// реализовано в скелете — оно появится вместе с событиями hover.
pub struct Tooltip;

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

        VElement::container(
            "",
            NodeKind::Column,
            vec![VElement::leaf(
                "",
                NodeKind::Text { content: text },
            )],
        )
    }
}

const TOOLTIP: CustomElementFactory = CustomElementFactory {
    tag: "tooltip",
    core_version_req: "^0.21",
    crate_name: "light-ui",
    provides: &["tooltip"],
    factory: || Box::new(Tooltip),
};

#[linkme::distributed_slice(light_fw_core::plugins::REGISTERED_COMPONENTS)]
static TOOLTIP_ENTRY: CustomElementFactory = TOOLTIP;
