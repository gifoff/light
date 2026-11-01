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
        // ТЗ 0.13-D-fix, задача 1.2: показать, что реально сериализует
        // `inspect_state` после клика (актуальный счётчик, а не устаревшее 0).
        eprintln!("INSPECT: {:?}", self.inspect_state());
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
    let app = App::new();
    // ТЗ 0.13-D-fix2, задача 2.2: одноразово показать реальный JSON снимка
    // (count: 1, а не Null). Отдельный `App` живёт только внутри спавна —
    // реактивный стор `Rc` (не `Send`) не переносится между потоками, поэтому
    // `app` не захватывается, а создаётся заново.
    let _handle = light::spawn(|_ctx| {
        let demo = App::default();
        demo.increment();
        eprintln!("INSPECT@START: {:?}", demo.inspect_state());
    });
    // Конфигурация берётся из `light.config.json`: в debug-сборке (профиль
    // `dev`) DevTools включается на порту 9229, в release (`prod`) — нет.
    light::run_with_config("light.config.json", app)
}
