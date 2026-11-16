//! Плагин-пример: кастомный элемент `<datepicker>`.
//!
//! Реализован через Plugin API (ТЗ 0.19.0 Этап 4):
//! * `#[component(tag = "datepicker")]` — генерирует `impl CustomElement` и
//!   фабрику в `REGISTERED_COMPONENTS`;
//! * `#[template("templates/datepicker.html")]` — HTML-шаблон элемента;
//! * `#[proxy]` — реактивные поля (`new()` из него использует фабрика).
//!
//! Чтобы использовать: добавьте крейт как зависимость и добавьте `<datepicker>`
//! в HTML приложения. Атрибуты и дети из HTML будут передаваться плагину при
//! runtime-рендеринге (ядро находит фабрику по тегу, вызовёт
//! `CustomElement::render`, перекрасит корень в `NodeKind::Custom("datepicker")`).

use light_fw::{component, proxy, template};

/// Календарь: показывает выбранный день или подсказку.
///
/// В Этапе 4 атрибуты и дочерние узлы не используются — рендер статичен.
#[component(tag = "datepicker")]
#[template("templates/datepicker.html")]
#[proxy]
pub struct Datepicker {
    /// Выбранный день в формате YYYY-MM-DD (или `None`).
    selected: Option<String>,
}

impl Datepicker {
    /// Клик по кнопке: ставит день «сегодня» (заглушка для Этапа 4).
    pub fn choose(&self) -> bool {
        self.set_selected(Some("2026-01-01".to_owned()));
        true
    }
}
