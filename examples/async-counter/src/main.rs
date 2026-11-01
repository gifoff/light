//! Асинхронное обновление состояния с возможностью отмены.
//!
//! Кнопка «Начать» запускает фоновую асинхронную задачу (через
//! `light::spawn_async` + `futures-timer`), которая имитирует долгую работу.
//! Пока задача выполняется, можно нажать «Отмена» — фоновая задача увидит
//! флаг отмены через `TaskContext` и завершится раньше, не обновляя состояние.
//!
//! Отмена кооперативная: `futures::executor::block_on` не поддерживает
//! принудительное прерывание, поэтому задача сама проверяет
//! `ctx.is_cancelled()` между шагами (здесь — после каждого асинхронного
//! `Delay`). Это устраняет busy-wait и зависание UI: поток блокируется на
//! таймере, а не в цикле `poll_fn`.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use futures_timer::Delay;

use light::prelude::*;

const TICKS: u32 = 20;
const TICK_MS: u64 = 100;

/// Обёртка над `Arc<AtomicBool>`.
///
/// `#[proxy]` автоматически выводит `Clone`, `Debug` и `PartialEq` для
/// всех полей структуры, а `Property::set` требует `T: PartialEq + Clone`.
/// Стандартная библиотека реализует эти трейты для `Arc<T>` только при
/// `T: PartialEq + Debug`, а `AtomicBool` — ни для того, ни для другого.
/// Поэтому флаг отмены хранится за newtype с ручными имплементациями:
/// `Clone` дёшевой (усиление `Arc`), `PartialEq` и `Debug` сравнить
/// указатель.
#[derive(Clone)]
pub struct CancelFlag(Arc<AtomicBool>);

impl serde::Serialize for CancelFlag {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.load(Ordering::Relaxed).serialize(serializer)
    }
}

impl std::fmt::Debug for CancelFlag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("CancelFlag")
            .field(&Arc::as_ptr(&self.0))
            .finish()
    }
}

impl PartialEq for CancelFlag {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

#[template("templates/app.html")]
#[proxy]
struct App {
    count: u32,
    loading: bool,
    /// Флаг отмены текущей фоновой задачи.
    ///
    /// Хранится в newtype, чтобы удовлетворить вывод `#[proxy]`.
    cancel_flag: Option<CancelFlag>,
}

impl App {
    fn start_async(&self) {
        if self.loading() {
            return;
        }

        self.set_loading(true);

        let delay = Duration::from_millis(TICK_MS);
        let handle = light::spawn_async(move |ctx| async move {
            // Асинхронная задержка: `Delay` блокирует поток на таймере
            // (без busy-wait), а после каждого тика проверяем флаг отмены.
            for _ in 0..TICKS {
                Delay::new(delay).await;
                if ctx.is_cancelled() {
                    return;
                }
            }

            // Задача завершилась без отмены — применяем результат.
            if !ctx.is_cancelled() {
                let _ = light::post_update::<App>(move |app| {
                    app.set_count(app.count() + 1);
                    app.set_loading(false);
                    app.set_cancel_flag(None);
                });
            }
        });

        // Флаг отмены, полученный от задачи, доживёт до отмены.
        self.set_cancel_flag(Some(CancelFlag(handle.cancel_flag())));
    }

    fn cancel_async(&self) {
        if let Some(flag) = self.cancel_flag.get() {
            flag.0.store(true, Ordering::SeqCst);
        }
        self.set_cancel_flag(None);
        self.set_loading(false);
    }
}

fn main() -> light::Result<()> {
    light::run(App::new())
}
