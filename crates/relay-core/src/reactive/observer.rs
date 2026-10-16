//! Центральный обозреватель реактивности (потоково-локальный).

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

thread_local! {
    static OBSERVER: RefCell<Observer> = RefCell::new(Observer::new());
}

/// Идентификатор реактивного свойства.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct PropertyId(u64);

/// Идентификатор эффекта — подписки наблюдателя [`watch`](super::watch).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct EffectId(u64);

struct Effect {
    callback: Rc<dyn Fn()>,
    running: bool,
}

/// Центральный реестр реактивности.
///
/// Доступ — через [`with_observer`].
pub struct Observer {
    next_property: u64,
    next_watcher: u64,
    active: Vec<EffectId>,
    subscribers: HashMap<PropertyId, HashSet<EffectId>>,
    deps: HashMap<EffectId, HashSet<PropertyId>>,
    effects: HashMap<EffectId, Effect>,
    repaint_requester: Option<Rc<dyn Fn()>>,
}

impl Observer {
    fn new() -> Self {
        Self {
            next_property: 0,
            next_watcher: 0,
            active: Vec::new(),
            subscribers: HashMap::new(),
            deps: HashMap::new(),
            effects: HashMap::new(),
            repaint_requester: None,
        }
    }

    /// Количество зарегистрированных наблюдателей.
    pub fn effect_count(&self) -> usize {
        self.effects.len()
    }

    /// Количество эффектов, подписанных на свойство.
    pub fn subscriber_count(&self, id: PropertyId) -> usize {
        self.subscribers.get(&id).map_or(0, HashSet::len)
    }

    fn unlink(&mut self, id: EffectId) {
        if let Some(deps) = self.deps.remove(&id) {
            for property in deps {
                let empty = match self.subscribers.get_mut(&property) {
                    Some(set) => {
                        set.remove(&id);
                        set.is_empty()
                    }
                    None => false,
                };
                if empty {
                    self.subscribers.remove(&property);
                }
            }
        }
    }
}

/// Выполняет замыкание с доступом к глобальному обозревателю.
///
/// Вложенные вызовы запрещены (произойдёт паника `RefCell`).
/// Основной способ доступа для интроспекции и тестов.
pub fn with_observer<R>(f: impl FnOnce(&mut Observer) -> R) -> R {
    OBSERVER.with(|observer| f(&mut observer.borrow_mut()))
}

/// Задаёт обработчик запроса перерисовки: вызывается при каждом
/// изменении реактивного свойства (обычно запрос перерисовки окна).
pub fn set_repaint_requester(f: impl Fn() + 'static) {
    with_observer(|observer| observer.repaint_requester = Some(Rc::new(f)));
}

pub(crate) fn register_property() -> PropertyId {
    with_observer(|observer| {
        observer.next_property += 1;
        PropertyId(observer.next_property)
    })
}

pub(crate) fn register_watcher(callback: Rc<dyn Fn()>) -> EffectId {
    with_observer(|observer| {
        observer.next_watcher += 1;
        let id = EffectId(observer.next_watcher);
        observer.effects.insert(
            id,
            Effect {
                callback,
                running: false,
            },
        );
        id
    })
}

pub(crate) fn remove_effect(id: EffectId) {
    with_observer(|observer| {
        observer.unlink(id);
        observer.effects.remove(&id);
    });
}

/// Фиксирует чтение свойства текущим эффектом.
pub(crate) fn track(property: PropertyId) {
    with_observer(|observer| {
        for index in 0..observer.active.len() {
            let id = observer.active[index];
            observer.deps.entry(id).or_default().insert(property);
            observer.subscribers.entry(property).or_default().insert(id);
        }
    });
}

/// Уведомляет об изменении свойства: перезапускает зависимых наблюдателей
/// и вызывает обработчик запроса перерисовки.
pub(crate) fn trigger(property: PropertyId) {
    let (to_run, requester) = with_observer(|observer| {
        let mut to_run = Vec::new();
        if let Some(subscribers) = observer.subscribers.get(&property) {
            for &id in subscribers {
                if let Some(effect) = observer.effects.get(&id)
                    && !effect.running
                {
                    to_run.push(id);
                }
            }
        }
        let requester = observer.repaint_requester.clone();
        (to_run, requester)
    });

    for id in to_run {
        run_watcher(id);
    }
    if let Some(requester) = requester {
        requester();
    }
}

/// Выполняет `f` как эффект `id`: читает свойства в его контексте,
/// очищая старые зависимости и записывая новые.
pub(crate) fn with_effect<R>(id: EffectId, f: impl FnOnce() -> R) -> R {
    with_observer(|observer| {
        observer.unlink(id);
        observer.active.push(id);
        if let Some(effect) = observer.effects.get_mut(&id) {
            effect.running = true;
        }
    });

    let result = f();

    with_observer(|observer| {
        debug_assert_eq!(observer.active.pop(), Some(id));
        if let Some(effect) = observer.effects.get_mut(&id) {
            effect.running = false;
        }
    });
    result
}

fn run_watcher(id: EffectId) {
    let callback = with_observer(|observer| {
        observer.effects.get(&id).and_then(|effect| {
            if effect.running {
                None
            } else {
                Some(Rc::clone(&effect.callback))
            }
        })
    });
    if let Some(callback) = callback {
        with_effect(id, move || callback());
    }
}
