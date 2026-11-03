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
}

impl App {
    fn increment(&self) -> bool {
        self.set_count(self.count() + 1);
        true
    }

    fn status(&self) -> String {
        if self.count() == 0 {
            "Кликните «Инкремент»".to_owned()
        } else {
            format!("Счётчик: {}", self.count())
        }
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
