//! Пример приложения с включённым TCP-сервером DevTools.
//!
//! Запускает окно со счётчиком и открывает TCP-порт для внешнего инспектора:
//! ```sh
//! cargo run --package example-devtools-demo
//! # затем в другом терминале:
//! # echo -n "SNAPSHOT\n" | nc 127.0.0.1 9229
//! ```

use light::prelude::*;

#[template("templates/app.html")]
#[proxy]
struct App {
    count: i64,
    // ТЗ 0.14.3-fix5: `#[transient]` — поле не сохраняется в state.json и не
    // восстанавливается при inject_state после hot-swap. Его значение всегда
    // берётся из нового кода (дефолт + расчёт в `increment`), поэтому после
    // swap status показывает текст «из нового кода», а не устаревший из state.
    #[transient]
    #[default("Кликните «Инкремент»".to_owned())]
    status: String,
}

impl App {
    fn increment(&self) -> bool {
        let n = self.count() + 1;
        self.set_count(n);
        let s = if n == 0 {
            "Кликните «Инкремент»".to_owned()
        } else {
            format!("НОВЫЙ ТЕКСТ: {n}")
        };
        self.set_status(s);
        true
    }

    fn minimize(&self) -> bool {
        if let Some(window) = Dom::window() {
            window.minimize();
        }
        true
    }

    fn close(&self) -> bool {
        if let Some(window) = Dom::window() {
            window.close();
        }
        true
    }
}

fn main() -> light::Result<()> {
    let app = App::new();
    // Конфигурация берётся из `light.config.json`: в debug-сборке (профиль
    // `dev`) DevTools включается на порту 9229, в release (`prod`) — нет.
    light::run_with_config("light.config.json", app)
}
