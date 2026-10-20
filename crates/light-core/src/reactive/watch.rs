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
/// ```no_run
/// # extern crate light_fw_core as light;
/// use light::prelude::*;
///
/// #[proxy]
/// struct Counter {
///     count: i64,
/// }
///
/// fn main() {
///     let counter = Counter::new();
///     let _watcher = watch({
///         let counter = counter.clone();
///         move || println!("count = {}", counter.count())
///     });
///     counter.set_count(1);
/// }
/// ```
pub fn watch(f: impl Fn() + 'static) -> Watcher {
    let callback: Rc<dyn Fn()> = Rc::new(f);
    let id = observer::register_watcher(Rc::clone(&callback));
    observer::with_effect(id, move || callback());
    Watcher { id, stopped: false }
}
