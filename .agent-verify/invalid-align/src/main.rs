//! Интеграционная проверка: невалидное значение `align-items` (через
//! динамическую привязку `:align-items="align_value"`) не должно падать с panic —
//! только warning в stderr и значение по умолчанию `start`.

use light::prelude::*;

#[template("templates/app.html")]
#[proxy]
struct App {
    align_value: String,
}

fn main() -> light::Result<()> {
    // Невалидное значение (опечатка), воспроизводящее исходный баг.
    let app = App::new();
    app.set_align_value("ыефке".to_owned());
    light::run_with_config("light.config.json", app)
}
