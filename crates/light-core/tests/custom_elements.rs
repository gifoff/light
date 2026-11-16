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
//!
//! Этап 5 — интеграция on_event: после парсинга `<datepicker>` в дереве
//! фиктивный плагин должен быть доступен через
//! [`drain_custom_instances`](light_fw_core::runtime::drain_custom_instances),
//! его `on_event` вызывается при клике (через оконный бэкенд), и плагин
//! может мутировать собственное состояние + запросить перерисовку.
#![cfg(feature = "runtime-templates")]

extern crate light_fw_core as light_fw;

use std::any::Any;

use light_fw::component::Component;
use light_fw::plugins::{CustomElement, REGISTERED_COMPONENTS, downcast_custom, lookup};
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
        core_version_req: "^0.19",
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
    assert_eq!(factory.core_version_req, "^0.19");
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

// ---------------------------------------------------------------------------
// Этап 5 — on_event dispatch: интеграция парсинга + реестра + мутирования
// ---------------------------------------------------------------------------

/// Фиктивный «datepicker» со счётчиком кликов (Этап 5).
///
/// `on_event` инкрементирует счётчик при `Click` и запрашивает перерисовку.
/// Хранится в `Rc<Cell<i32>>`, потому что плагин создаётся через `fn() -> Box<dyn CustomElement>`
/// (копию нельзя передать из фабрики), а между рендерами ядро хранит
/// отдельный экземпляр, поэтому каждый «кадр» видит свой собственный счётчик.
struct ClickCounter {
    count: std::cell::Cell<i32>,
}

impl ClickCounter {
    fn new() -> Self {
        Self {
            count: std::cell::Cell::new(0),
        }
    }
}

impl CustomElement for ClickCounter {
    fn tag(&self) -> &'static str
    where
        Self: Sized,
    {
        "click-counter"
    }

    fn render(&self, _attrs: &[(&'static str, String)], _children: &[VElement]) -> VElement {
        // Колонка с одним Text — «подложка» для внутреннего кнопочного элемента.
        // Ключи дочерних узлов назначаются runtime (0.0, 0.1, …), поэтому
        // dispatch по ключу "0.0" (внутренний Text) поднимется до "0" (наш
        // Custom) и найдёт этот элемент.
        VElement::container(
            "",
            NodeKind::Column,
            vec![VElement::leaf(
                "",
                NodeKind::Text {
                    content: format!("Clicks: {}", self.count.get()),
                },
            )],
        )
    }

    fn on_event(&mut self, event: &Event, ctx: &mut light_fw::plugins::EventCtx<'_>) {
        if matches!(event, Event::Click) {
            self.count.set(self.count.get() + 1);
            // Запрашиваем перерисовку — после on_event бэкенд вызовет
            // invalidate(), если флаг стоит.
            ctx.schedule_redraw();
        }
    }
}

/// Регистрация фиктивного «click-counter» в distributed-slice.
#[linkme::distributed_slice(REGISTERED_COMPONENTS)]
static CLICK_COUNTER_ENTRY: light_fw::plugins::CustomElementFactory =
    light_fw::plugins::CustomElementFactory {
        tag: "click-counter",
        core_version_req: "^0.19",
        crate_name: "click-counter-test",
        provides: &["click-counter"],
        factory: || Box::new(ClickCounter::new()),
    };

/// Интеграционный тест Этапа 5:
///
/// 1. `parse_template("<click-counter />")` вызывает `build_custom_element`,
///    который сохраняет новый экземпляр в thread-local реестр.
/// 2. `drain_custom_instances` выносит его (очищая карту).
/// 3. Эмулируем то, что делает `WindowState::dispatch_custom_event`:
///    вызываем `on_event` на задрейненном экземпляре с `Event::Click`,
///    проверяем, что счётчик увеличился и `schedule_redraw` был вызван.
#[test]
fn stage5_click_counter_on_event_integration() {
    let c = NullComponent;
    let Some(v) = light_fw::runtime::parse_template("<click-counter />", &c) else {
        panic!("parse_template(<click-counter />) обязан вернуть Some");
    };
    let custom_node = find_custom(&v).expect("в дереве должен быть Custom('click-counter')");
    assert!(matches!(
        &custom_node.kind,
        NodeKind::Custom(tag) if *tag == "click-counter"
    ));

    // Выносим экземпляр из реестра (это делает WindowState::redraw).
    let mut map = light_fw::runtime::drain_custom_instances();
    assert!(!map.is_empty(), "реестр обязан содержать click-counter");
    let key = map
        .keys()
        .next()
        .cloned()
        .expect("должен быть хотя бы один элемент");
    let mut inst = map.remove(&key).expect("элемент обязан быть в карте");

    // До клика счётчик = 0.
    assert_eq!(
        downcast_custom::<ClickCounter>(&*inst)
            .expect("должен быть ClickCounter")
            .count
            .get(),
        0
    );

    // Эмулируем клик.
    let redraw_flag = std::cell::Cell::new(false);
    let redraw = || {
        redraw_flag.set(true);
    };
    let mut ctx = light_fw::plugins::EventCtx::new(&redraw);
    inst.on_event(&Event::Click, &mut ctx);
    map.insert(key, inst);

    // После клика: счётчик = 1, redraw запрошен.
    let cc = downcast_custom::<ClickCounter>(&**map.values().next().unwrap())
        .expect("должен быть ClickCounter");
    assert_eq!(
        cc.count.get(),
        1,
        "счётчик после одного клика обязан быть 1"
    );
    assert!(redraw_flag.get(), "schedule_redraw обязан быть вызван");
}

/// Несколько кликов → счётчик растёт.
#[test]
fn stage5_click_counter_multiple_clicks() {
    let c = NullComponent;
    let _ = light_fw::runtime::parse_template("<click-counter />", &c)
        .expect("parse обязан вернуть Some");
    let mut map = light_fw::runtime::drain_custom_instances();
    let key = map
        .keys()
        .next()
        .cloned()
        .expect("должен быть хотя бы один элемент");
    let mut inst = map.remove(&key).expect("элемент обязан быть в карте");

    for i in 0..3 {
        let flag = std::cell::Cell::new(false);
        let flag_ref = &flag;
        let redraw = || {
            flag_ref.set(true);
        };
        let mut ctx = light_fw::plugins::EventCtx::new(&redraw);
        inst.on_event(&Event::Click, &mut ctx);
        assert!(flag.get(), "клик #{i} обязан запрашивать перерисовку");
    }
    let cc = downcast_custom::<ClickCounter>(&*inst).expect("должен быть ClickCounter");
    assert_eq!(cc.count.get(), 3, "после 3 кликов счётчик обязан быть 3");
}

/// Не-кликовые события НЕ увеличивают счётчик.
#[test]
fn stage5_non_click_events_do_not_increment() {
    let c = NullComponent;
    let _ = light_fw::runtime::parse_template("<click-counter />", &c)
        .expect("parse обязан вернуть Some");
    let mut map = light_fw::runtime::drain_custom_instances();
    let key = map
        .keys()
        .next()
        .cloned()
        .expect("должен быть хотя бы один элемент");
    let mut inst = map.remove(&key).expect("элемент обязан быть в карте");

    for ev in [
        Event::Mousedown(light_fw::vdom::MouseButton::Left),
        Event::Mouseup(light_fw::vdom::MouseButton::Left),
        Event::Dblclick,
    ] {
        let flag = std::cell::Cell::new(false);
        let flag_ref = &flag;
        let redraw = || {
            flag_ref.set(true);
        };
        let mut ctx = light_fw::plugins::EventCtx::new(&redraw);
        inst.on_event(&ev, &mut ctx);
        let cc = downcast_custom::<ClickCounter>(&*inst).expect("должен быть ClickCounter");
        assert_eq!(
            cc.count.get(),
            0,
            "событие {ev:?} не обязано увеличивать счётчик"
        );
        assert!(
            !flag.get(),
            "событие {ev:?} не обязано запрашивать перерисовку"
        );
    }
}

/// Псевдоним для compile-check.
#[allow(dead_code)]
type EventCtx<'a> = light_fw::plugins::EventCtx<'a>;
