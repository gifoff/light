//! Тесты реактивного ядра: свойства, наблюдатели и запросы перерисовки.

extern crate relay_core as relay;

use std::cell::Cell;
use std::rc::Rc;

use relay::prelude::*;
use relay::reactive::{set_repaint_requester, with_observer};

#[proxy]
struct State {
    count: i64,
    #[default("мир".to_owned())]
    label: String,
}

fn repaint_counter() -> Rc<Cell<u32>> {
    let repaints = Rc::new(Cell::new(0));
    let counter = repaints.clone();
    set_repaint_requester(move || counter.set(counter.get() + 1));
    repaints
}

#[test]
fn setting_the_same_value_does_not_notify() {
    let state = State::new();
    let repaints = repaint_counter();

    state.set_count(7);
    assert_eq!(repaints.get(), 1, "0 -> 7 меняется");
    state.set_count(7);
    assert_eq!(repaints.get(), 1, "запись того же значения не уведомляет");
}

#[test]
fn change_always_requests_repaint() {
    let state = State::new();
    let repaints = repaint_counter();

    state.set_count(1);
    assert_eq!(
        repaints.get(),
        1,
        "запрос перерисовки выполняется даже без подписчиков"
    );
}

#[test]
fn watcher_subscribes_reruns_and_stops_cleanly() {
    let state = State::new();
    let runs = Rc::new(Cell::new(0));

    let mut watcher = watch({
        let state = state.clone();
        let runs = runs.clone();
        move || {
            let _count = state.count();
            runs.set(runs.get() + 1);
        }
    });
    assert_eq!(runs.get(), 1, "наблюдатель выполняется сразу");
    assert_eq!(with_observer(|observer| observer.effect_count()), 1);
    assert_eq!(
        with_observer(|observer| observer.subscriber_count(state.count_property().id())),
        1,
        "наблюдатель подписан на прочитанное свойство"
    );

    state.set_count(1);
    assert_eq!(runs.get(), 2, "наблюдатель перезапускается");
    state.set_count(1);
    assert_eq!(runs.get(), 2, "то же значение — без перезапуска");

    watcher.stop();
    assert_eq!(with_observer(|observer| observer.effect_count()), 0);
    assert_eq!(
        with_observer(|observer| observer.subscriber_count(state.count_property().id())),
        0,
        "подписки сняты при отписке"
    );
    state.set_count(5);
    assert_eq!(runs.get(), 2, "остановленный наблюдатель не вызывается");
    watcher.stop();
}

#[test]
fn update_and_invalidate_always_notify() {
    let state = State::new();
    let repaints = repaint_counter();

    state.count_property().update(|count| *count += 1);
    assert_eq!(repaints.get(), 1, "update всегда уведомляет");

    state.invalidate();
    assert_eq!(
        repaints.get(),
        3,
        "invalidate уведомляет по одному разу на каждое свойство"
    );
    assert_eq!(state.count(), 1);
}

#[test]
fn cloned_proxy_shares_state() {
    let state = State::new();
    let clone = state.clone();
    state.set_count(3);
    assert_eq!(clone.count(), 3, "клон — тот же поток данных");
    clone.set_label("мир!".to_owned());
    assert_eq!(state.label(), "мир!");
}

#[test]
fn property_equality_compares_values() {
    let first = State::new();
    let second = State::new();
    assert_eq!(first, second, "равные значения — равные прокси");

    first.set_count(1);
    assert_ne!(first, second, "разные значения — неравные прокси");
}
