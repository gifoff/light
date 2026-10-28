//! Асинхронное обновление состояния с возможностью отмены.
//!
//! Кнопка «Начать» запускает фоновую задачу, которая имитирует долгую
//! работу (200 шагов по 5 мс). Пока задача выполняется, можно нажать
//! «Отмена» — фоновая задача увидит флаг отмены и завершится раньше,
//! не обновляя состояние.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use light::prelude::*;

const TICKS: u32 = 200;
const TICK_MS: u64 = 5;

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

        let next = self.count() + 1;
        let handle = light::spawn(move |ctx| {
            for _ in 0..TICKS {
                if ctx.is_cancelled() {
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(TICK_MS));
            }

            // Задача завершилась без отмены — применяем результат.
            let _ = light::post_update::<App>(move |app| {
                app.set_count(next);
                app.set_loading(false);
                app.set_cancel_flag(None);
            });
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
