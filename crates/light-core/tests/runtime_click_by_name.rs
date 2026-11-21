//! Регрессия 0.20.1 (баг 1): клик по runtime-кнопке без явного `key=` должен
//! доставляться обработчику по ИМЕНИ метода `@click`.
//!
//! Runtime-дерево (`light dev` / `render_dynamic`) ключирует кнопку, у которой
//! нет атрибута `key=`, по нормализованному имени обработчика (`@click="increment"`
//! → key `"increment"`). Команда DevTools `click --key increment` отправляет
//! `CLICK "increment"` в `dispatch`. До 0.20.1 генерируемый макросом
//! `invoke_handler` сопоставлял ПОЗИЦИОННЫЙ статический ключ (`"0.2"`), отбрасывая
//! имя, поэтому `dispatch("increment", Click)` проваливался в `_ => false`, а
//! `click --key increment` возвращал `ok:false` (срабатывало только `--key 0.2`).
//!
//! Тест проверяет полный путь `dispatch` → `dispatch_by_name` → `invoke_handler`
//! на макросно-сгенерированном компоненте (`#[template]` + `#[proxy]`), то есть
//! ровно тот код, который исполняет реальный `gui-app`.

#![cfg(feature = "runtime-templates")]

extern crate light_fw_core as light_fw;

use light_fw::component::Component;
use light_fw::vdom::{Event, VElement};
use light_fw::{proxy, template};

/// Компонент повторяет `gui-app`: поле `count` и метод `increment`. Шаблон
/// содержит кнопку БЕЗ явного `key=` — runtime сгенерирует ей key `"increment"`.
#[template("tests/templates/counter_no_key.html")]
#[proxy]
struct CountApp {
    count: i64,
}

impl CountApp {
    fn increment(&self) -> bool {
        self.set_count(self.count() + 1);
        true
    }
}

/// Кнопка без `key=`: `find --tag button` вернёт key `"increment"`; клик по этому
/// key должен дойти до `increment` (state вырастает), а клик по чужому имени — нет.
#[test]
fn default_button_clicks_by_handler_name() {
    let app = CountApp::new();

    // Дерево, которое нарисует GUI: кнопка имеет key = имя обработчика.
    let tree = app.render();
    let key = find_button_key(&tree, "Click").expect("кнопка «Click»");
    assert_eq!(
        key, "increment",
        "runtime-кнопка без key должна получать имя обработчика"
    );

    // Клик по имени (то, что шлёт `light devtools click --key increment`).
    assert!(
        app.dispatch("increment", Event::Click),
        "клик по имени обработчика должен быть обработан"
    );
    assert_eq!(app.count(), 1, "@click вызван по имени");

    // Повторный клик — состояние продолжает расти.
    assert!(app.dispatch("increment", Event::Click));
    assert_eq!(app.count(), 2);

    // Чужее имя — обработчик не вызывается.
    assert!(
        !app.dispatch("no_such_handler", Event::Click),
        "чужое имя не должно вызывать обработчик"
    );
    assert_eq!(app.count(), 2, "состояние не изменилось после чужого клика");
}

/// Явный `key="inc_btn"` сохраняет адресацию через `invoke_handler` по ключу узла
/// (regression guard: правка бага 1 не ломает путь `explicit_key_addresses_dispatch`).
#[template("tests/templates/explicit_key.html")]
#[proxy]
struct ExplicitKeyApp {
    value: i64,
}

impl ExplicitKeyApp {
    fn increment(&self) -> bool {
        self.set_value(self.value() + 1);
        true
    }
}

#[test]
fn explicit_key_still_addressable_by_name() {
    let app = ExplicitKeyApp::new();

    // Кнопка с явным `key="inc_btn"`: runtime-ключ равен явному значению.
    let tree = app.render();
    let key = find_button_key(&tree, "Инкремент").expect("кнопка «Инкремент»");
    assert_eq!(key, "inc_btn");

    // Диспетчеризация по явному ключу (имя не совпадает — идёт через invoke_handler
    // по ключу узла / явную ветку dispatch).
    assert!(
        app.dispatch("inc_btn", Event::Click),
        "клик по явному ключу инкрементирует"
    );
    assert_eq!(app.value(), 1);
}

/// Возвращает ключ контейнера-кнопки, содержащего текст `content`.
fn find_button_key(node: &VElement, content: &str) -> Option<&'static str> {
    if matches!(node.kind, light_fw::vdom::NodeKind::Button { .. })
        && node.children.iter().any(
            |c| matches!(&c.kind, light_fw::vdom::NodeKind::Text { content: t } if t == content),
        )
    {
        return Some(node.key);
    }
    node.children
        .iter()
        .find_map(|c| find_button_key(c, content))
}

/// Регрессия бага 2: при включённой фиче `runtime-templates` рендер идёт через
/// `render_dynamic` (runtime-парсер), и текст `{{ count }}` должен обновляться
/// после клика (0 → 1). До правки клик по имени не доставлялся, поэтому состояние
/// не менялось и отрисованный текст оставался прежним (скриншоты одинаковые).
#[test]
fn runtime_text_updates_after_click() {
    let app = CountApp::new();

    // Начальное дерево: текст «0».
    let tree0 = app.render();
    let text0 = find_first_text(&tree0).expect("текстовый узел {{ count }}");
    assert_eq!(
        text0, "0",
        "интерполяция должна показать начальное значение"
    );

    // Клик по имени обработчика (то, что шлёт `light devtools click --key increment`).
    assert!(
        app.dispatch("increment", Event::Click),
        "клик по имени должен инкрементировать count"
    );

    // Перерисовка: runtime-рендер вычисляет `{{ count }}` заново → «1».
    let tree1 = app.render();
    let text1 = find_first_text(&tree1).expect("текстовый узел {{ count }}");
    assert_eq!(
        text1, "1",
        "после клика интерполяция должна обновиться 0 → 1"
    );
}

/// Первый найденный текстовый узел (его содержимое как `&str`).
fn find_first_text<'a>(node: &'a VElement) -> Option<&'a str> {
    if let light_fw::vdom::NodeKind::Text { content } = &node.kind {
        return Some(content.as_str());
    }
    node.children.iter().find_map(find_first_text)
}
