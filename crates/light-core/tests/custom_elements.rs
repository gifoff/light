//! Интеграционные тесты Plugin API (Этап 3): регистрация кастомных
//! элементов через distributed-slice и их рендер через
//! [`light_fw_core::runtime::parse_template`].
//!
//! Фиктивный «datepicker» регистрируется в модуле ниже через
//! `#[linkme::distributed_slice(REGISTERED_COMPONENTS)]`. Этот бинарник
//! линкует distributed-slice, поэтому [`lookup`](light_fw_core::plugins::lookup)
//! находит фабрику по тегу, а runtime-путь
//! [`parse_template`](light_fw_core::runtime::parse_template)
//! вызывает `render` и оборачивает результат в
//! [`NodeKind::Custom`](light_fw_core::vdom::NodeKind::Custom).
#![cfg(feature = "runtime-templates")]

extern crate light_fw_core as light_fw;

use std::any::Any;

use light_fw::component::Component;
use light_fw::plugins::{CustomElement, REGISTERED_COMPONENTS, lookup};
use light_fw::vdom::{Event, NodeKind, VElement};

/// Тестовый компонент без полей и реактивности.
struct NullComponent;

impl Component for NullComponent {
    fn render(&self) -> VElement {
        VElement::container("", NodeKind::Column, Vec::new())
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// Фиктивный «datepicker»: возвращает Column с одним Text «DatePicker».
///
/// Без состояния — достаточно для проверки, что runtime действительно
/// вызывает `render` и оборачивает результат в `NodeKind::Custom`.
struct FakeDatepicker;

impl CustomElement for FakeDatepicker {
    fn tag(&self) -> &'static str
    where
        Self: Sized,
    {
        "datepicker"
    }

    fn render(&self, _attrs: &[(&'static str, String)], _children: &[VElement]) -> VElement {
        VElement::container(
            "",
            NodeKind::Column,
            vec![VElement::leaf(
                "",
                NodeKind::Text {
                    content: "DatePicker".to_string(),
                },
            )],
        )
    }
}

/// Регистрация фиктивного плагина в distributed-slice.
///
/// Каждый бинарник (тест, пример, реальное приложение) содержит свою копию
/// slice — здесь он добавляет фабрику. Сборка линкует все слои, и ядро
/// находит фабрику по тегу.
#[linkme::distributed_slice(REGISTERED_COMPONENTS)]
static FAKE_DATEPICKER_ENTRY: light_fw::plugins::CustomElementFactory =
    light_fw::plugins::CustomElementFactory {
        tag: "datepicker",
        core_version_req: "^0.18",
        crate_name: "fake-datepicker-test",
        provides: &["datepicker"],
        factory: || Box::new(FakeDatepicker),
    };

// ---------------------------------------------------------------------------
// Lookup: фабрика видна ядру
// ---------------------------------------------------------------------------

/// Фабрика «datepicker» видна после линковки distributed-slice.
#[test]
fn lookup_finds_fake_datepicker() {
    let factory = lookup("datepicker").expect("datepicker должен быть зарегистрирован");
    assert_eq!(factory.tag, "datepicker");
    assert_eq!(factory.crate_name, "fake-datepicker-test");
    assert_eq!(factory.provides, &["datepicker"]);
    assert_eq!(factory.core_version_req, "^0.18");
}

/// Незарегистрированный тег → None.
#[test]
fn lookup_unknown_returns_none() {
    assert!(lookup("totally-unknown-tag").is_none());
}

// ---------------------------------------------------------------------------
// parse_template: неизвестный тег
// ---------------------------------------------------------------------------

/// Неподдержанный тег → parse_template вернёт None (parse_fail).
#[test]
fn unregistered_tag_gives_none() {
    let c = NullComponent;
    let result =
        light_fw::runtime::parse_template("<totally-unknown-tag>hi</totally-unknown-tag>", &c);
    assert!(
        result.is_none(),
        "незарегистрированный тег обязан давать None"
    );
}

// ---------------------------------------------------------------------------
// parse_template: зарегистрированный «datepicker»
// ---------------------------------------------------------------------------

/// `<datepicker>` → Some(Custom("datepicker")) с одним Text-дочерником
/// «DatePicker».
#[test]
fn registered_datepicker_renders_custom() {
    let c = NullComponent;
    let Some(v) = light_fw::runtime::parse_template("<datepicker />", &c) else {
        panic!("parse_template(<datepicker />) должен вернуть Some");
    };
    // Корневой узел — контейнер Column, внутри которого — наш Custom.
    // (parse_template оборачивает одиночный элемент в колонку, если он не <window>.)
    match v.kind {
        NodeKind::Custom(_) => {}
        NodeKind::Column if !v.children.is_empty() => {}
        other => panic!("ожидался Column или Custom, получен: {other:?}"),
    }
    let custom_node = find_custom(&v).expect("в поддереве должен быть Custom('datepicker')");
    assert!(
        matches!(&custom_node.kind, NodeKind::Custom(tag) if *tag == "datepicker"),
        "кастомный узел должен иметь тег datepicker, actual: {:?}",
        custom_node.kind,
    );
    assert_eq!(custom_node.children.len(), 1);
    match &custom_node.children[0].kind {
        NodeKind::Text { content } => {
            assert_eq!(content, "DatePicker");
        }
        other => panic!("ожидался Text 'DatePicker', получен: {other:?}"),
    }
}

/// `<datepicker>` с атрибутами — атрибуты передаются плагину (здесь просто
/// проверяется, что рендер всё ещё работает).
#[test]
fn registered_datepicker_with_attrs_renders() {
    let c = NullComponent;
    let html = r#"<datepicker format="YYYY-MM-DD" placeholder="Pick a date" />"#;
    let Some(v) = light_fw::runtime::parse_template(html, &c) else {
        panic!("parse_template с атрибутами обязан вернуть Some");
    };
    assert!(
        find_custom(&v).is_some(),
        "кастомный узел datepicker должен быть в дереве"
    );
}

// ---------------------------------------------------------------------------
// Вспомогательные
// ---------------------------------------------------------------------------

/// Рекурсивный поиск первого узла с `NodeKind::Custom(_)` в поддереве.
fn find_custom(node: &VElement) -> Option<&VElement> {
    if matches!(node.kind, NodeKind::Custom(_)) {
        return Some(node);
    }
    node.children.iter().find_map(find_custom)
}

/// Тест, что [`on_event`](light_fw::plugins::CustomElement::on_event) имеет
/// дефолтную реализацию (compile-only check).
#[allow(dead_code)]
fn _default_on_event_compiles(element: &mut FakeDatepicker, event: &Event, ctx: &mut EventCtx<'_>) {
    element.on_event(event, ctx);
}

/// Псевдоним для читаемости compile-check.
#[allow(dead_code)]
type EventCtx<'a> = light_fw::plugins::EventCtx<'a>;
