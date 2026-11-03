//! Пример: два независимых окна с асинхронными обновлениями, адресованными
//! к конкретному окну через [`light::post_update_to`].
//!
//! Корневым окном является [`Main`] (видимое окно «Окно 1»); кнопка
//! «Открыть Окно 2» отправляет `WindowCommand::OpenWindow`, и бэкенд
//! открывает второе главное окно — компонент [`Second`], возвращаемый
//! `Component::window_factory`. Второе окно не имеет родителя: приложение
//! завершается только когда закрыты оба главных окна.
//!
//! Каждое окно — самостоятельный компонент со своим счётчиком и своей
//! асинхронной задачей. Обновления в одном окне не влияют на второе:
//! каждая итерация цикла отправляется только в целевое окно, идентифицированное
//! его [`WindowToken`] (полученным через [`light::current_window_token`]).
//!
//! Запуск:
//! ```bash
//! cargo run -p example-multi-window
//! ```

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use light::async_runtime::post_update_to;
use light::control::{WindowCommand, request};
use light::prelude::*;
use light::{TaskContext, current_window_token, spawn};

/// Обёртка вокруг `Arc<AtomicBool>` для отмены задачи.
///
/// `Clone` — копирование `Arc` (дешёвое). `PartialEq` — сравнение
/// по указателю на внутреннюю `Arc` (не по значению флага).
/// `Debug` — выводим как `CancelFlag { ... }`.
#[derive(Debug, Clone)]
pub struct CancelFlag(Arc<AtomicBool>);

impl PartialEq for CancelFlag {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Default for CancelFlag {
    fn default() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }
}

impl CancelFlag {
    fn set_cancel(&self, val: bool) {
        self.0.store(val, Ordering::Relaxed);
    }

    fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// Окно 1 (корневое): заголовок, счётчик, асинхронная кнопка и кнопка
/// «Открыть Окно 2».
///
/// Все асинхронные обновления адресованы к этому окну через
/// [`post_update_to`] с токеном, полученным в момент нажатия кнопки.
///
/// Аргумент `window = Second` макроса `#[template]` генерирует
/// `Component::window_factory`, которая открывает второе главное окно
/// ([`Second`]) по `WindowCommand::OpenWindow`.
#[template("templates/app.html", window = Second)]
#[proxy]
struct Main {
    /// Название окна.
    #[default(String::from("Окно 1"))]
    title: String,

    /// Счётчик, который обновляется по кнопке.
    #[default(0)]
    count: u32,

    /// Флаг: асинхронная задача запущена (кнопка активна).
    #[default(false)]
    async_button: bool,

    /// Флаг отмены текущей асинхронной задачи (None — нет задачи).
    #[default(None)]
    cancel_flag: Option<CancelFlag>,
}

impl Main {
    /// Увеличение счётчика (вызывается из асинхронной задачи).
    #[allow(dead_code)]
    pub fn increment(&self) -> bool {
        self.set_count(self.count.get() + 1);
        true
    }

    /// Открывает второе главное окно ([`Second`]) через
    /// `WindowCommand::OpenWindow` (использует `Component::window_factory`).
    pub fn open_second(&self) -> bool {
        request(WindowCommand::OpenWindow);
        true
    }

    /// Запуск асинхронной задачи: 30 итераций × 100 мс (≈ 3 с).
    /// Счётчик увеличивается ровно на 1 после завершения цикла.
    /// Все обновления идут только в это окно через `post_update_to`
    /// с токеном текущего окна.
    pub fn start_async(&self) -> bool {
        // Если уже есть активная задача — отменяем её и не запускаем новую.
        if let Some(existing) = self.cancel_flag.get().clone() {
            existing.set_cancel(true);
            return true;
        }

        // Создаём флаг отмены и сохраняем его в компонент.
        let flag = CancelFlag::default();
        self.set_cancel_flag(Some(flag.clone()));
        self.set_async_button(true);

        // Токен текущего окна — фиксируется в момент нажатия кнопки.
        let token = match current_window_token() {
            Some(t) => t,
            None => {
                eprintln!("async-runtime: current_window_token() == None in Main::start_async");
                return true;
            }
        };

        // Клон для передачи в задачу (Arc — дешёвое копирование).
        let flag_for_task = flag.clone();

        // Запускаем задачу в runtime.
        let _ = spawn(move |ctx: TaskContext| {
            // Ждём 3 секунды (30 раз по 100 мс).
            for _ in 0..30 {
                if flag_for_task.is_cancelled() || ctx.is_cancelled() {
                    let _ = post_update_to::<Main>(token, move |m: &mut Main| {
                        m.set_async_button(false);
                        m.set_cancel_flag(None);
                    });
                    return;
                }

                std::thread::sleep(Duration::from_millis(100));
            }

            // Счётчик увеличивается ровно на 1 — один раз после цикла.
            let _ = post_update_to::<Main>(token, move |m: &mut Main| {
                m.set_count(m.count.get() + 1);
                m.set_async_button(false);
                m.set_cancel_flag(None);
            });
        });
        true
    }

    /// Загрузка: есть ли активный асинхронный процесс.
    #[allow(dead_code)]
    pub fn loading(&self) -> bool {
        self.cancel_flag.get().is_some()
    }

    /// Кнопка активна (флаг установлен).
    #[allow(dead_code)]
    pub fn start(&self) -> bool {
        self.async_button.get()
    }

    /// Есть ли флаг отмены (для отображения кнопки «Отмена»).
    #[allow(dead_code)]
    pub fn has_cancel_flag(&self) -> bool {
        self.cancel_flag.get().is_some()
    }

    /// Получить флаг для передачи в callback (копирование `Arc`).
    #[allow(dead_code)]
    pub fn cancel_flag_value(&self) -> Option<CancelFlag> {
        self.cancel_flag.get().clone()
    }

    /// Отмена текущей асинхронной задачи.
    #[allow(dead_code)]
    pub fn cancel_async(&self) -> bool {
        if let Some(flag) = self.cancel_flag.get().clone() {
            flag.set_cancel(true);
        }
        true
    }
}

/// Окно 2: заголовок, счётчик, асинхронная кнопка.
///
/// Точно такая же логика, как в `Main`, но с другим шаблоном и
/// другим токеном окна. Обновления здесь не затрагивают Окно 1.
#[template("templates/app2.html")]
#[proxy]
struct Second {
    /// Название окна.
    #[default(String::from("Окно 2"))]
    title: String,

    /// Счётчик, который обновляется по кнопке.
    #[default(0)]
    count: u32,

    /// Флаг: асинхронная задача запущена (кнопка активна).
    #[default(false)]
    async_button: bool,

    /// Флаг отмены текущей асинхронной задачи (None — нет задачи).
    #[default(None)]
    cancel_flag: Option<CancelFlag>,
}

impl Second {
    /// Увеличение счётчика по кнопке.
    #[allow(dead_code)]
    pub fn increment(&self) -> bool {
        self.set_count(self.count.get() + 1);
        true
    }

    /// Запуск асинхронной задачи: 30 итераций × 100 мс (≈ 3 с).
    /// Счётчик увеличивается ровно на 1 после завершения цикла.
    /// Все обновления идут только в это окно через `post_update_to`
    /// с токеном текущего окна.
    #[allow(dead_code)]
    pub fn start_async(&self) -> bool {
        // Если уже есть активная задача — отменяем её и не запускаем новую.
        if let Some(existing) = self.cancel_flag.get().clone() {
            existing.set_cancel(true);
            return true;
        }

        // Создаём флаг отмены и сохраняем его в компонент.
        let flag = CancelFlag::default();
        self.set_cancel_flag(Some(flag.clone()));
        self.set_async_button(true);

        // Токен текущего окна — фиксируется в момент нажатия кнопки.
        let token = match current_window_token() {
            Some(t) => t,
            None => {
                eprintln!("async-runtime: current_window_token() == None in Second::start_async");
                return true;
            }
        };

        // Клон для передачи в задачу (Arc — дешёвое копирование).
        let flag_for_task = flag.clone();

        // Запускаем задачу в runtime.
        let _ = spawn(move |ctx: TaskContext| {
            // Ждём 3 секунды (30 раз по 100 мс).
            for _ in 0..30 {
                if flag_for_task.is_cancelled() || ctx.is_cancelled() {
                    let _ = post_update_to::<Second>(token, move |m: &mut Second| {
                        m.set_async_button(false);
                        m.set_cancel_flag(None);
                    });
                    return;
                }

                std::thread::sleep(Duration::from_millis(100));
            }

            // Счётчик увеличивается ровно на 1 — один раз после цикла.
            let _ = post_update_to::<Second>(token, move |m: &mut Second| {
                m.set_count(m.count.get() + 1);
                m.set_async_button(false);
                m.set_cancel_flag(None);
            });
        });
        true
    }

    /// Загрузка: есть ли активный асинхронный процесс.
    #[allow(dead_code)]
    pub fn loading(&self) -> bool {
        self.cancel_flag.get().is_some()
    }

    /// Кнопка активна (флаг установлен).
    #[allow(dead_code)]
    pub fn start(&self) -> bool {
        self.async_button.get()
    }

    /// Есть ли флаг отмены (для отображения кнопки «Отмена»).
    #[allow(dead_code)]
    pub fn has_cancel_flag(&self) -> bool {
        self.cancel_flag.get().is_some()
    }

    /// Получить флаг для передачи в callback (копирование `Arc`).
    #[allow(dead_code)]
    pub fn cancel_flag_value(&self) -> Option<CancelFlag> {
        self.cancel_flag.get().clone()
    }

    /// Отмена текущей асинхронной задачи.
    #[allow(dead_code)]
    pub fn cancel_async(&self) -> bool {
        if let Some(flag) = self.cancel_flag.get().clone() {
            flag.set_cancel(true);
        }
        true
    }
}

fn main() {
    let _ = light::run(Main::default());
}
