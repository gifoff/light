//! Сквозные тесты без окна: HTML-шаблон, директивы, события и `<include>`.
//!
//! Каждый тест строит дерево, доставляет событие и проверяет через
//! [`diff`], что обновились **только** изменившиеся элементы.

use relay::prelude::*;
use relay::vdom::{Event, NodeKind, Op, Size, VElement, diff};

/// Компонент из `templates/counter.html`.
#[template("tests/templates/counter.html")]
#[proxy]
struct CounterApp {
    label: String,
    count: i64,
}

impl CounterApp {
    fn increment(&self) {
        self.set_count(self.count() + 1);
    }

    fn reset(&self) {
        self.set_count(0);
    }

    /// Кнопка «+1» блокируется при достижении максимума.
    fn is_max(&self) -> bool {
        self.count() >= 2
    }
}

/// Ключ узла по отображаемому тексту или подписи.
///
/// Подпись кнопки-контейнера хранится в текстовом ребёнке, но события
/// адресуются самой кнопке, поэтому для кнопок возвращается ключ контейнера.
fn key_of(tree: &VElement, content: &str) -> &'static str {
    fn find_button(node: &VElement, content: &str) -> Option<&'static str> {
        if matches!(node.kind, NodeKind::Button { .. })
            && node.children.iter().any(
                |child| matches!(&child.kind, NodeKind::Text { content: text } if text == content),
            )
        {
            return Some(node.key);
        }
        node.children
            .iter()
            .find_map(|child| find_button(child, content))
    }
    find_button(tree, content)
        .or_else(|| tree.find_by_content(content).map(|node| node.key))
        .unwrap_or_else(|| panic!("узел «{content}» не найден"))
}

/// Первое текстовое поле в дереве (обход в глубину).
fn find_field(tree: &VElement) -> &VElement {
    fn search(node: &VElement) -> Option<&VElement> {
        if matches!(node.kind, NodeKind::TextField { .. }) {
            return Some(node);
        }
        node.children.iter().find_map(search)
    }
    search(tree).expect("в дереве нет текстового поля")
}

#[test]
fn template_renders_initial_state() {
    let app = CounterApp::new();
    let tree = app.render();

    tree.find_by_content("Значение: 0")
        .expect("текст отражает счётчик");
    tree.find_by_content("+1").expect("кнопка +1");
    tree.find_by_content("Сброс").expect("кнопка Сброс");
    let field = find_field(&tree);
    assert!(matches!(
        field.kind,
        NodeKind::TextField {
            ref value,
            disabled: false,
            ..
        } if value.is_empty()
    ));
}

#[test]
fn click_updates_property_and_patch_touches_only_text() {
    let app = CounterApp::new();
    let before = app.render();
    let text_key = key_of(&before, "Значение: 0");

    app.dispatch(key_of(&before, "+1"), Event::Click);
    assert_eq!(app.count(), 1, "обработчик @click вызван");

    let after = app.render();
    let patch = diff(&before, &after);
    assert_eq!(
        patch.ops,
        vec![Op::SetContent {
            key: text_key,
            content: "Значение: 1".to_owned(),
        }],
        "после клика обновляется только текстовый узел"
    );
}

#[test]
fn disabled_directive_flips_button_flag() {
    let app = CounterApp::new();
    let before = app.render();
    let plus_key = key_of(&before, "+1");
    let text_key = key_of(&before, "Значение: 0");

    app.dispatch(plus_key, Event::Click);
    app.dispatch(plus_key, Event::Click);
    assert_eq!(app.count(), 2);

    let after = app.render();
    let patch = diff(&before, &after);
    assert_eq!(
        patch.ops,
        vec![
            Op::SetContent {
                key: text_key,
                content: "Значение: 2".to_owned(),
            },
            Op::SetDisabled {
                key: plus_key,
                disabled: true,
            },
        ],
        "при достижении максимума меняется только подпись и флаг кнопки"
    );
}

#[test]
fn v_model_dispatch_writes_property_back() {
    let app = CounterApp::new();
    let before = app.render();
    let field_key = find_field(&before).key;

    app.dispatch(field_key, Event::TextChange("Привет".to_owned()));
    assert_eq!(app.label(), "Привет", "v-model записывает в свойство");

    let after = app.render();
    let patch = diff(&before, &after);
    assert_eq!(
        patch.ops,
        vec![Op::SetValue {
            key: field_key,
            value: "Привет".to_owned(),
        }],
        "обновляется только текстовое поле"
    );
}

/// Компонент из `templates/directives.html`: `<template>`, цепочки
/// `v-if`/`v-else-if`/`v-else` и `v-for`.
#[template("tests/templates/directives.html")]
#[proxy]
struct DirectivesApp {
    #[default(true)]
    empty: bool,
    single: bool,
    #[default(true)]
    show_list: bool,
    items: Vec<String>,
}

#[test]
fn template_renders_fragment_without_wrapper() {
    let app = DirectivesApp::new();
    let tree = app.render();
    let root = tree.get("").expect("корневой контейнер");
    assert_eq!(
        root.children.len(),
        2,
        "два корневых <div> без узла <template>"
    );
    tree.find_by_content("пусто")
        .expect("v-if принимает <template>");
}

#[test]
fn v_else_chain_switches_branch() {
    let app = DirectivesApp::new();
    app.set_empty(false);
    app.set_single(true);
    let tree = app.render();
    assert!(
        tree.find_by_content("один").is_some(),
        "сработала v-else-if"
    );
    assert!(tree.find_by_content("пусто").is_none());

    app.set_empty(false);
    app.set_single(false);
    let tree = app.render();
    assert!(
        tree.find_by_content("много").is_some(),
        "сработала ветка v-else"
    );
    assert!(tree.find_by_content("пусто").is_none());
    assert!(tree.find_by_content("один").is_none());
}

#[test]
fn v_for_repeats_nodes_with_unique_keys() {
    let app = DirectivesApp::new();
    let tree = app.render();
    let root = tree.get("").expect("корневой контейнер");
    let list = &root.children[1];
    assert_eq!(list.children.len(), 0, "список пуст при пустом items");

    app.set_items(vec!["А".to_owned(), "Б".to_owned()]);
    let tree = app.render();
    let root = tree.get("").expect("корневой контейнер");
    let list = &root.children[1];
    assert_eq!(list.children.len(), 2, "по одному узлу на элемент v-for");
    tree.find_by_content("А")
        .expect("интерполяция {{ item }} работает");
    assert_ne!(
        list.children[0].key, list.children[1].key,
        "ключи элементов списка разные"
    );
}

/// Компонент из `templates/toggle.html`: `v-if` и `:disabled` на поле.
#[template("tests/templates/toggle.html")]
#[proxy]
struct ToggleApp {
    #[default(true)]
    visible: bool,
    locked: bool,
    note: String,
}

impl ToggleApp {
    fn hide(&self) {
        self.set_visible(false);
    }

    fn toggle_lock(&self) {
        self.set_locked(!self.locked());
    }
}

#[test]
fn v_if_hides_elements_and_patch_removes_them() {
    let app = ToggleApp::new();
    let before = app.render();
    before.find_by_content("Видно").expect("текст виден");
    before.find_by_content("Скрыть").expect("кнопка видна");

    app.dispatch(key_of(&before, "Скрыть"), Event::Click);
    assert!(!app.visible(), "обработчик @click изменил свойство");

    let after = app.render();
    assert!(after.find_by_content("Видно").is_none(), "v-if скрыл текст");
    assert!(
        after.find_by_content("Скрыть").is_none(),
        "v-if скрыл кнопку"
    );
    let patch = diff(&before, &after);
    assert_eq!(
        patch.ops,
        vec![Op::Remove { key: "0.0" }, Op::Remove { key: "0.1" }],
        "структурное изменение — только удаление скрытых узлов"
    );
}

#[test]
fn disabled_textfield_reflects_property() {
    let app = ToggleApp::new();
    let before = app.render();
    let field_key = find_field(&before).key;

    app.dispatch(key_of(&before, "Замок"), Event::Click);
    assert!(app.locked(), "поле заблокировано");

    let after = app.render();
    let patch = diff(&before, &after);
    assert_eq!(
        patch.ops,
        vec![Op::SetDisabled {
            key: field_key,
            disabled: true,
        }],
        "обновляется только флаг текстового поля"
    );
}

/// Компонент из `templates/pages/main.html` с include-секциями.
#[template("tests/templates/pages/main.html")]
#[proxy]
struct PagesApp {
    #[default("home".to_owned())]
    page: String,
}

impl PagesApp {
    fn is_home(&self) -> bool {
        self.page() == "home"
    }

    fn is_about(&self) -> bool {
        self.page() == "about"
    }

    fn show_about(&self) {
        self.set_page("about".to_owned());
    }
}

#[test]
fn include_splices_sections_into_main_form() {
    let app = PagesApp::new();
    let tree = app.render();

    tree.find_by_content("Основная форма")
        .expect("корень главной формы");
    tree.find_by_content("Добро пожаловать!")
        .expect("вложенный include внутри секции home");
    tree.find_by_content("Это главная страница.")
        .expect("секция home вставлена");
    assert!(
        tree.find_by_content("Раздел «О проекте».").is_none(),
        "v-if скрывает неактивную секцию"
    );
}

#[test]
fn switching_page_patches_only_sections() {
    let app = PagesApp::new();
    let before = app.render();

    app.show_about();
    let after = app.render();
    let patch = diff(&before, &after);

    assert!(
        patch
            .ops
            .iter()
            .any(|op| matches!(op, Op::Insert { key: "0.2" })),
        "подключается секция about: {:?}",
        patch.ops
    );
    assert!(
        patch
            .ops
            .iter()
            .any(|op| matches!(op, Op::Remove { key: "0.1" })),
        "удаляется секция home: {:?}",
        patch.ops
    );
    after
        .find_by_content("Раздел «О проекте».")
        .expect("about активна");
    assert!(
        after.find_by_content("Добро пожаловать!").is_none(),
        "home скрыта"
    );
}

/// Компонент из `templates/styled.html`: атрибуты стиля.
#[template("tests/templates/styled.html")]
#[proxy]
struct StyledApp {
    ready: bool,
    depth: i32,
}

#[test]
fn style_attributes_are_compiled_into_vnode() {
    let app = StyledApp::new();
    app.set_depth(5);
    let tree = app.render();

    let div = tree.get("0").expect("корневой <div>");
    assert_eq!(div.style.width, Size::Percent(50.0), "width в процентах");
    assert_eq!(
        div.style.padding,
        relay::vdom::Edges {
            top: 10.0,
            right: 10.0,
            bottom: 10.0,
            left: 20.0,
        },
        "padding-left переопределяет сторону"
    );
    assert_eq!(
        div.style.margin,
        relay::vdom::Edges {
            top: 7.0,
            right: 4.0,
            bottom: 4.0,
            left: 4.0,
        },
        "margin-top переопределяет сторону"
    );
    assert_eq!(div.style.background, Some(0xFFFF_FFFF));
    assert_eq!(div.style.border_size, 2.0);
    assert_eq!(div.style.border_color, Some(0x0000_00FF));
    assert_eq!(
        div.style.align_x,
        relay::vdom::Align::End,
        "position-x=right"
    );
    assert_eq!(
        div.style.align_y,
        relay::vdom::Align::End,
        "position-y=bottom"
    );
    assert_eq!(
        div.style.offset_x,
        relay::vdom::Offset::Percent(10.0),
        "position-offset-x в процентах"
    );
    assert_eq!(
        div.style.offset_y,
        relay::vdom::Offset::Px(-5.0),
        "position-offset-y отрицательный"
    );
    assert_eq!(div.style.z, 3, "статический z");

    let span = tree.get("0.0").expect("вложенный <span>");
    assert_eq!(span.style.font_size, Some(20.0));
    assert_eq!(span.style.font_color, Some(0xFF00_00FF));
    assert_eq!(
        span.style.offset_x,
        relay::vdom::Offset::Px(3.0),
        "position-offset-x"
    );
    assert_eq!(span.style.z, 5, "динамический :z=\"depth\"");
    assert_eq!(
        span.style.justify_content,
        relay::vdom::Align::End,
        "justify-content=end"
    );
    assert_eq!(
        span.style.align_items,
        relay::vdom::Align::Center,
        "align-items=center"
    );
    assert!(
        tree.find_by_content("Стиль").is_some(),
        "голый текст внутри <span>"
    );
}

/// Компонент из `templates/maximize.html`: переключение кнопок по состоянию окна.
#[template("tests/templates/maximize.html")]
#[proxy]
struct MaximizeApp {
    ready: bool,
}

#[test]
fn maximize_state_switches_button() {
    relay::control::set_maximized(false);
    let app = MaximizeApp::new();
    let before = app.render();
    before
        .find_by_content("Развернуть")
        .expect("кнопка развернуть");
    assert!(before.find_by_content("Восстановить").is_none());

    relay::control::set_maximized(true);
    let after = app.render();
    after
        .find_by_content("Восстановить")
        .expect("кнопка восстановить");
    assert!(after.find_by_content("Развернуть").is_none());

    relay::control::set_maximized(false);
}

/// Компонент из `templates/events.html`: обработчики событий мыши и клавиатуры.
#[template("tests/templates/events.html")]
#[proxy]
struct EventsApp {
    mouse: String,
    key: String,
}

impl EventsApp {
    fn click(&self) {
        self.set_mouse("click".to_owned());
    }

    fn double_click(&self) {
        self.set_mouse("dblclick".to_owned());
    }

    fn down(&self, button: MouseButton) {
        self.set_mouse(format!("down:{button:?}"));
    }

    fn up(&self, button: MouseButton) {
        self.set_mouse(format!("up:{button:?}"));
    }

    fn mv(&self, x: f32, y: f32) {
        self.set_mouse(format!("move:{x},{y}"));
    }

    fn enter(&self) {
        self.set_mouse("enter".to_owned());
    }

    fn leave(&self) {
        self.set_mouse("leave".to_owned());
    }

    fn key_down(&self, info: KeyInfo) {
        self.set_key(format!("down:{}", info.key));
    }

    fn key_up(&self, info: KeyInfo) {
        self.set_key(format!("up:{}", info.key));
    }
}

#[test]
fn events_dispatch_to_handlers() {
    let app = EventsApp::new();
    let tree = app.render();
    let area = key_of(&tree, "Область");

    app.dispatch(area, Event::Click);
    assert_eq!(app.mouse(), "click");
    app.dispatch(area, Event::Dblclick);
    assert_eq!(app.mouse(), "dblclick");
    app.dispatch(area, Event::Mousedown(MouseButton::Left));
    assert_eq!(app.mouse(), "down:Left");
    app.dispatch(area, Event::Mouseup(MouseButton::Right));
    assert_eq!(app.mouse(), "up:Right");
    app.dispatch(area, Event::Mousemove { x: 3.0, y: 4.0 });
    assert_eq!(app.mouse(), "move:3,4");
    app.dispatch(area, Event::Mouseenter);
    assert_eq!(app.mouse(), "enter");
    app.dispatch(area, Event::Mouseleave);
    assert_eq!(app.mouse(), "leave");

    app.dispatch(
        area,
        Event::Keydown(KeyInfo::new("Enter", false, false, false)),
    );
    assert_eq!(app.key(), "down:Enter");
    app.dispatch(
        area,
        Event::Keyup(KeyInfo::new("Escape", false, true, false)),
    );
    assert_eq!(app.key(), "up:Escape");
}

/// Дочерний компонент модального окна из `templates/modal_child.html`.
#[template("tests/templates/modal_child.html")]
#[proxy]
struct ModalChild {
    ready: bool,
}

/// Родительский компонент из `templates/modal_parent.html` с `modal = ModalChild`.
#[template("tests/templates/modal_parent.html", modal = ModalChild)]
#[proxy]
struct ModalParent {
    ready: bool,
}

#[test]
fn modal_component_is_provided_by_template_arg() {
    let app = ModalParent::new();
    let parent = app.window_config().expect("окно родителя");
    assert_eq!(parent.width, 200);

    let child = app.modal().expect("модальный компонент задан");
    let child_window = child.window_config().expect("окно ребёнка");
    assert_eq!(child_window.width, 120);
    assert!(child_window.modal, "ребёнок помечен модальным");
    assert!(
        ModalChild::new().modal().is_none(),
        "у ребёнка собственного модала нет"
    );
}

#[test]
fn open_modal_without_backend_is_noop() {
    let app = ModalParent::new();
    let tree = app.render();
    let button = key_of(&tree, "Открыть");
    assert!(
        app.dispatch(button, Event::Click),
        "обработчик @click считается обработанным"
    );
}

/// Компонент из `templates/visible.html` с динамическим `:visible`.
#[template("tests/templates/visible.html")]
#[proxy]
struct VisibleApp {
    shown: bool,
}

impl VisibleApp {
    fn toggle(&self) {
        self.set_shown(!self.shown());
    }
}

#[test]
fn visible_binding_marks_hidden_node() {
    let app = VisibleApp::new();
    let tree = app.render();
    let span = &tree.children[0].children[1];
    assert!(!span.style.visible, "по умолчанию узел скрыт");

    let button = key_of(&tree, "Переключить");
    app.dispatch(button, Event::Click);
    let tree = app.render();
    let span = &tree.children[0].children[1];
    assert!(span.style.visible, ":visible включает узел");
}

/// Компонент из `templates/hidden_window.html` с `visible="false"`.
#[template("tests/templates/hidden_window.html")]
#[proxy]
struct HiddenWindowApp {
    ready: bool,
}

#[test]
fn window_visible_attribute_is_parsed() {
    let app = HiddenWindowApp::new();
    let config = app.window_config().expect("окно");
    assert!(!config.visible, "visible='false' скрывает окно");
}

/// Первый узел, удовлетворяющий предикату, в обходе в глубину.
fn find_kind<'a>(
    tree: &'a VElement,
    predicate: &dyn Fn(&NodeKind) -> bool,
) -> Option<&'a VElement> {
    if predicate(&tree.kind) {
        return Some(tree);
    }
    tree.children
        .iter()
        .find_map(|child| find_kind(child, predicate))
}

/// Компонент из `templates/forms.html`: флажок, переключатели, список и пароль.
#[template("tests/templates/forms.html")]
#[proxy]
struct FormsApp {
    agreed: bool,
    plan: String,
    city: String,
    secret: String,
}

#[test]
fn form_elements_render_and_bind() {
    let app = FormsApp::new();
    let before = app.render();

    let checkbox = find_kind(&before, &|kind| matches!(kind, NodeKind::Checkbox { .. }))
        .expect("флажок отрисован");
    let NodeKind::Checkbox {
        label,
        checked,
        disabled,
    } = &checkbox.kind
    else {
        unreachable!()
    };
    assert_eq!(label, "Согласен");
    assert!(!checked, "флажок по умолчанию не отмечен");
    assert!(!disabled);
    let checkbox_key = checkbox.key;

    let radio = find_kind(
        &before,
        &|kind| matches!(kind, NodeKind::Radio { value, .. } if value == "a"),
    )
    .expect("переключатель A отрисован");
    let radio_key = radio.key;
    assert!(matches!(
        &radio.kind,
        NodeKind::Radio { label, selected: false, .. } if label == "Вариант A"
    ));

    let select = find_kind(&before, &|kind| matches!(kind, NodeKind::Select { .. }))
        .expect("список отрисован");
    let select_key = select.key;
    let NodeKind::Select { value, options, .. } = &select.kind else {
        unreachable!()
    };
    assert!(value.is_empty(), "выбор пуст по умолчанию");
    assert_eq!(options.len(), 2);
    assert_eq!(options[1].label, "Питер");
    assert_eq!(options[1].value, "spb");

    let password = find_kind(&before, &|kind| {
        matches!(kind, NodeKind::TextField { masked: true, .. })
    })
    .expect("поле пароля отрисовано");
    let password_key = password.key;
    assert!(matches!(
        &password.kind,
        NodeKind::TextField { placeholder, .. } if placeholder == "Пароль"
    ));

    app.dispatch(checkbox_key, Event::Click);
    assert!(app.agreed(), "клик по флажку меняет булево свойство");
    let after = app.render();
    assert_eq!(
        diff(&before, &after).ops,
        vec![Op::SetChecked {
            key: checkbox_key,
            checked: true,
        }],
        "флажок обновляется точечно"
    );

    app.dispatch(radio_key, Event::Click);
    assert_eq!(app.plan(), "a", "клик по переключателю пишет его value");
    let after_radio = app.render();
    assert_eq!(
        diff(&after, &after_radio).ops,
        vec![Op::SetSelected {
            key: radio_key,
            selected: true,
        }],
        "переключатель обновляется точечно"
    );

    app.dispatch(select_key, Event::TextChange("spb".to_owned()));
    assert_eq!(app.city(), "spb", "выбор пункта пишет value");
    let after_select = app.render();
    assert_eq!(
        diff(&after_radio, &after_select).ops,
        vec![Op::SetValue {
            key: select_key,
            value: "spb".to_owned(),
        }],
        "список обновляется точечно"
    );

    app.dispatch(password_key, Event::TextChange("hunter2".to_owned()));
    assert_eq!(app.secret(), "hunter2", "поле пароля пишется как обычное");
}

#[test]
fn form_nodes_expose_tag_and_attributes() {
    let app = FormsApp::new();
    let tree = app.render();

    let checkbox =
        find_kind(&tree, &|kind| matches!(kind, NodeKind::Checkbox { .. })).expect("флажок");
    assert_eq!(checkbox.tag, "checkbox");
    assert_eq!(checkbox.attr("v-model"), None, "директивы не сохраняются");

    let radio = find_kind(
        &tree,
        &|kind| matches!(kind, NodeKind::Radio { value, .. } if value == "a"),
    )
    .expect("переключатель");
    assert_eq!(radio.tag, "radio");
    assert_eq!(radio.attr("value"), Some("a"));

    let password = find_kind(&tree, &|kind| {
        matches!(kind, NodeKind::TextField { masked: true, .. })
    })
    .expect("пароль");
    assert_eq!(password.tag, "password");
    assert_eq!(password.attr("placeholder"), Some("Пароль"));

    let select = find_kind(&tree, &|kind| matches!(kind, NodeKind::Select { .. })).expect("список");
    assert_eq!(select.tag, "select");
}

/// Компонент из `templates/headings.html` с `<h3>`/`<h4>`/`<h5>`.
#[template("tests/templates/headings.html")]
#[proxy]
struct HeadingsApp {
    ready: bool,
}

#[test]
fn headings_h3_h4_h5_set_default_font_size() {
    let app = HeadingsApp::new();
    let tree = app.render();

    let size = |tag: &str| tree.find_by_tag(tag).and_then(|node| node.style.font_size);
    assert_eq!(size("h3"), Some(20.0));
    assert_eq!(size("h4"), Some(18.0));
    assert_eq!(size("h5"), Some(16.0));
    tree.find_by_content("Три").expect("текст заголовка");
    tree.find_by_content("Пять").expect("текст заголовка");
}

/// Дочернее окно второго главного окна из `templates/window_child.html`.
#[template("tests/templates/window_child.html")]
#[proxy]
struct WindowChild {
    ready: bool,
}

/// Родитель из `templates/window_parent.html` с `window = WindowChild`.
#[template("tests/templates/window_parent.html", window = WindowChild)]
#[proxy]
struct WindowParent {
    ready: bool,
}

#[test]
fn window_factory_is_provided_by_template_arg() {
    let parent = WindowParent::new();
    let extra = parent
        .window_factory()
        .expect("window_factory задан аргументом `window =`");
    let config = extra.window_config().expect("окно второго компонента");
    assert_eq!(config.width, 300, "размеры берутся из шаблона");
    assert!(!config.modal, "второе главное окно не модальное");
}

#[test]
fn window_factory_defaults_to_none() {
    let app = CounterApp::new();
    assert!(
        app.window_factory().is_none(),
        "без аргумента `window =` дополнительное окно не задано"
    );
}

/// Дочернее окно из `templates/window_settings.html` с `id` и `parent`.
#[template("tests/templates/window_settings.html")]
#[proxy]
struct WindowSettings {
    ready: bool,
}

/// Родитель из `templates/windows_parent.html` со списком
/// `windows = [WindowSettings]`.
#[template("tests/templates/windows_parent.html", windows = [WindowSettings])]
#[proxy]
struct WindowsParent {
    ready: bool,
}

#[test]
fn window_id_and_parent_are_parsed() {
    let parent = WindowsParent::new();
    let config = parent.window_config().expect("окно родителя");
    assert_eq!(config.id.as_deref(), Some("main"), "id из <window id=…>");
    assert!(config.parent.is_none(), "у корня нет родителя");

    let child = WindowSettings::new();
    let config = child.window_config().expect("окно ребёнка");
    assert_eq!(config.id.as_deref(), Some("settings"));
    assert_eq!(config.parent.as_deref(), Some("main"));
}

#[test]
fn windows_list_provides_factories() {
    let parent = WindowsParent::new();
    let factories = parent.windows();
    assert_eq!(
        factories.len(),
        1,
        "одна фабрика из windows = [WindowSettings]"
    );
    let child = factories[0]();
    assert_eq!(
        child
            .window_config()
            .and_then(|config| config.id)
            .as_deref(),
        Some("settings"),
        "фабрика создаёт окно с ожидаемым id"
    );
    assert!(
        CounterApp::new().windows().is_empty(),
        "без аргумента windows список пуст"
    );
}

#[test]
fn open_id_handler_parses_string_argument() {
    let app = WindowsParent::new();
    let tree = app.render();
    let button = key_of(&tree, "Открыть настройки");
    assert!(
        app.dispatch(button, Event::Click),
        "обработчик open('settings') вызывается без паники"
    );
}

/// Компонент из `templates/a11y.html` с атрибутами доступности.
#[template("tests/templates/a11y.html")]
#[proxy]
struct A11yApp {
    name: String,
    agree: bool,
    hidden: bool,
}

#[test]
fn aria_attributes_are_compiled_into_nodes() {
    let app = A11yApp::new();
    let tree = app.render();

    let heading = tree.find_by_tag("h1").expect("заголовок отрисован");
    assert_eq!(heading.attr("aria-label"), Some("Заголовок формы"));

    let field = find_kind(&tree, &|kind| matches!(kind, NodeKind::TextField { .. }))
        .expect("текстовое поле отрисовано");
    assert_eq!(field.attr("aria-label"), Some("Имя пользователя"));
    assert_eq!(field.attr("aria-describedby"), Some("hint"));
    assert_eq!(
        field.attr("required"),
        Some(""),
        "булев атрибут хранится пустой строкой"
    );
    assert!(
        tree.find_by_attr("id", "hint").is_some(),
        "узел-описание доступен по id"
    );

    let item = tree
        .find_by_attr("role", "list-item")
        .expect("явная роль сохранена");
    assert_eq!(item.tag, "div", "роль не подменяет тег");
}

#[test]
fn dynamic_aria_binding_reflects_property() {
    let app = A11yApp::new();
    let before = app.render();
    let span = before
        .find_by_attr("id", "secret")
        .expect("узел с id=secret");
    assert_eq!(
        span.attr("aria-hidden"),
        Some("false"),
        "динамический aria-hidden вычисляется"
    );

    app.set_hidden(true);
    let after = app.render();
    let span = after
        .find_by_attr("id", "secret")
        .expect("узел с id=secret");
    assert_eq!(span.attr("aria-hidden"), Some("true"));
}

/// Проверяет, что готовое дерево доступности содержит ожидаемую семантику.
#[cfg(feature = "a11y")]
#[test]
fn accessibility_tree_from_template() {
    use relay::a11y::tree::build_update;
    use relay::prelude::AriaRole;

    let app = A11yApp::new();
    let tree = app.render();
    let update = build_update(&tree, None);

    let find = |role: AriaRole| {
        update
            .nodes
            .iter()
            .find(|(_, node)| node.role() == role)
            .map(|(_, node)| node)
    };

    let heading = find(AriaRole::Heading).expect("заголовок в дереве доступности");
    assert_eq!(heading.label(), Some("Заголовок формы"));
    assert_eq!(heading.level(), Some(1));

    let field = find(AriaRole::TextInput).expect("поле ввода в дереве доступности");
    assert_eq!(field.label(), Some("Имя пользователя"));
    assert!(field.is_required());
    assert!(
        !field.described_by().is_empty(),
        "описание связано через aria-describedby"
    );

    assert!(find(AriaRole::CheckBox).is_some());
    assert!(find(AriaRole::ListItem).is_some());
    assert!(
        find(AriaRole::Button).expect("кнопка").is_disabled(),
        "aria-disabled отражается как Disabled"
    );
}

/// Компонент из `templates/dynamic.html`: динамические привязки `:attr`.
#[template("tests/templates/dynamic.html")]
#[proxy]
struct DynamicApp {
    w: i64,
    h: String,
    px: String,
    py: String,
    off_x: i64,
    off_y: String,
    justify: String,
    align_items: String,
    pad: i64,
    margin: i64,
    border: i64,
    font: i64,
    bg: String,
    fg: String,
    bc: String,
}

/// Динамические `:attr` переопределяют статические значения стиля.
#[test]
fn dynamic_style_attrs_bind_values() {
    use relay::vdom::{Align, Offset, Size};
    let app = DynamicApp::new();
    app.set_w(200);
    app.set_h("50%".to_owned());
    app.set_px("right".to_owned());
    app.set_py("bottom".to_owned());
    app.set_off_x(-8);
    app.set_off_y("10%".to_owned());
    app.set_justify("end".to_owned());
    app.set_align_items("center".to_owned());
    app.set_pad(7);
    app.set_margin(3);
    app.set_border(2);
    app.set_font(20);
    app.set_bg("#112233".to_owned());
    app.set_fg("#445566".to_owned());
    app.set_bc("#778899".to_owned());
    let tree = app.render();
    let div = tree.get("0").expect("div");
    let style = div.style;
    assert_eq!(style.width, Size::Px(200.0));
    assert_eq!(style.height, Size::Percent(50.0));
    assert_eq!(style.align_x, Align::End);
    assert_eq!(style.align_y, Align::End);
    assert_eq!(style.offset_x, Offset::Px(-8.0));
    assert_eq!(style.offset_y, Offset::Percent(10.0));
    assert_eq!(style.justify_content, Align::End);
    assert_eq!(style.align_items, Align::Center);
    assert_eq!(style.padding, relay::vdom::Edges::all(7.0));
    assert_eq!(style.margin, relay::vdom::Edges::all(3.0));
    assert_eq!(style.border_size, 2.0);
    assert_eq!(style.font_size, Some(20.0));
    assert_eq!(style.background, Some(0x112233FF));
    assert_eq!(style.font_color, Some(0x445566FF));
    assert_eq!(style.border_color, Some(0x778899FF));
    assert_eq!(div.attr("width"), Some("200"));
}

/// Компонент из `templates/dynamic_kinds.html`: динамические `:attr` у типов узлов.
#[template("tests/templates/dynamic_kinds.html")]
#[proxy]
struct DynamicKindsApp {
    kind: String,
    locked: bool,
    name: String,
    hint: String,
    multi: bool,
    resize_mode: String,
    choice: String,
    radio_value: String,
    opt_value: String,
}

/// Динамические `:attr` работают для `NodeKind`: `type`, `placeholder`,
/// `textarea`, `resize`, `value`.
#[test]
fn dynamic_node_kind_attrs_bind_values() {
    use relay::vdom::{ButtonType, NodeKind, Resize};
    let app = DynamicKindsApp::new();
    app.set_kind("reset".to_owned());
    app.set_locked(true);
    app.set_hint("Подсказка".to_owned());
    app.set_multi(true);
    app.set_resize_mode("both".to_owned());
    app.set_choice("b".to_owned());
    app.set_radio_value("a".to_owned());
    app.set_opt_value("o1".to_owned());
    let tree = app.render();

    let button = tree.find_by_tag("button").expect("button");
    assert!(matches!(
        button.kind,
        NodeKind::Button {
            disabled: true,
            button_type: ButtonType::Reset,
        }
    ));

    let field = find_field(&tree);
    match &field.kind {
        NodeKind::TextField {
            placeholder,
            multiline,
            resize,
            ..
        } => {
            assert_eq!(placeholder, "Подсказка");
            assert!(*multiline);
            assert_eq!(*resize, Resize::Both);
        }
        other => panic!("ожидалось поле, найдено {other:?}"),
    }

    let radio = tree.find_by_tag("radio").expect("radio");
    match &radio.kind {
        NodeKind::Radio {
            value, selected, ..
        } => {
            assert_eq!(value, "a");
            assert!(!selected);
        }
        other => panic!("ожидалось радио, найдено {other:?}"),
    }

    let select = tree.find_by_tag("select").expect("select");
    match &select.kind {
        NodeKind::Select { options, .. } => {
            assert_eq!(options[0].value, "o1");
        }
        other => panic!("ожидался список, найдено {other:?}"),
    }
}
