//! Реактивное свойство — ячейка значения с уведомлением обозревателей.

use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

use super::observer::{self, PropertyId};

/// Реактивное свойство (аналог `ref` в Vue).
///
/// Чтение через [`get`](Self::get) фиксируется обозревателем, если выполняется
/// внутри эффекта (подписки [`watch`](super::watch)). Запись через
/// [`set`](Self::set) уведомляет все зависимые эффекты и запрашивает перерисовку.
///
/// Значение хранится в `Rc<RefCell<_>>`, поэтому клонирование [`Property`]
/// разделяет одну и ту же ячейку (как и клон реактивной ссылки в Vue).
/// Потокобезопасность соответствует одно поточному UI: использовать только
/// в потоке отрисовки.
pub struct Property<T> {
    id: PropertyId,
    value: Rc<RefCell<T>>,
}

impl<T> Property<T> {
    /// Создаёт свойство и регистрирует его у обозревателя.
    pub fn new(value: T) -> Self {
        Self {
            id: observer::register_property(),
            value: Rc::new(RefCell::new(value)),
        }
    }

    /// Идентификатор свойства у обозревателя.
    pub fn id(&self) -> PropertyId {
        self.id
    }

    /// Чтение без отслеживания зависимостей.
    pub fn peek(&self) -> T
    where
        T: Clone,
    {
        self.value.borrow().clone()
    }

    /// Чтение с отслеживанием зависимостей (если выполняется внутри эффекта).
    pub fn get(&self) -> T
    where
        T: Clone,
    {
        observer::track(self.id);
        self.value.borrow().clone()
    }

    /// Запись нового значения; уведомляет обозревателей, если значение изменилось.
    pub fn set(&self, value: T)
    where
        T: PartialEq,
    {
        let changed = {
            let mut cell = self.value.borrow_mut();
            if *cell == value {
                false
            } else {
                *cell = value;
                true
            }
        };
        if changed {
            observer::trigger(self.id);
        }
    }

    /// Изменение значения через замыкание; всегда уведомляет обозревателей.
    pub fn update(&self, f: impl FnOnce(&mut T)) {
        {
            let mut cell = self.value.borrow_mut();
            f(&mut cell);
        }
        observer::trigger(self.id);
    }
}

impl<T> Clone for Property<T> {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            value: Rc::clone(&self.value),
        }
    }
}

impl<T: PartialEq> PartialEq for Property<T> {
    /// Свойства равны, если равны их значения (идентификаторы не учитываются).
    fn eq(&self, other: &Self) -> bool {
        *self.value.borrow() == *other.value.borrow()
    }
}

impl<T> fmt::Debug for Property<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Property")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}
