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
    fn increment(&self) {
        self.set_count(self.count() + 1);
    }

    fn status(&self) -> String {
        if self.count() == 0 {
            "Кликните «Инкремент»".to_owned()
        } else {
            format!("Счётчик: {}", self.count())
        }
    }
}

fn main() -> light::Result<()> {
    // Запускаем TCP-сервер DevTools; при ошибке (порт занят и т.п.)
    // сообщаем и выходим.
    let port = match light::enable_devtools(9229) {
        Ok(port) => port,
        Err(e) => {
            eprintln!("Не удалось запустить DevTools сервер на порту 9229: {e}");
            std::process::exit(1);
        }
    };
    eprintln!("DevTools listening on 127.0.0.1:{port}");
    light::run(App::new())
}
