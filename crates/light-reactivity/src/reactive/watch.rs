//! Наблюдатель — подписка на изменение реактивных свойств.

use std::rc::Rc;

use super::observer::{self, EffectId};

/// Подписка ([`watch`]), переживающая до [`stop`](Self::stop) или удаления.
///
/// Чтения, выполненные обработчиком, запоминаются автоматически: при изменении
/// любого из свойств обработчик выполняется заново.
pub struct Watcher {
    id: EffectId,
    stopped: bool,
}

impl Watcher {
    /// Идентификатор эффекта наблюдателя.
    pub fn id(&self) -> EffectId {
        self.id
    }

    /// Отписывает обработчик и удаляет его зависимости.
    pub fn stop(&mut self) {
        if !self.stopped {
            self.stopped = true;
            observer::remove_effect(self.id);
        }
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Создаёт наблюдателя (аналог `watchEffect` в Vue): выполняет `f` один раз,
/// запоминает прочитанные свойства и повторяет `f` при их изменении.
///
/// Пример с `Property`:
///
/// ```
/// use light_reactivity::{Property, watch};
///
/// let prop = Property::new(0i64);
/// let mut watcher = watch({
///     let prop = prop.clone();
///     move || {
///         // любое чтение внутри обработчика фиксируется как зависимость
///         let _ = prop.get();
///     }
/// });
/// assert_eq!(prop.get(), 0);
/// watcher.stop();
/// ```
pub fn watch(f: impl Fn() + 'static) -> Watcher {
    let callback: Rc<dyn Fn()> = Rc::new(f);
    let id = observer::register_watcher(Rc::clone(&callback));
    observer::with_effect(id, move || callback());
    Watcher { id, stopped: false }
}
