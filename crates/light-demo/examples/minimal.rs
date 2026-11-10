//! Минимальный пример: только базовые элементы. Служит эталоном для замера
//! размера бинарника при разных наборах фич (см. `scripts/bloat.ps1`).

use light_fw::prelude::*;

/// Компонент из `examples/templates/minimal.html`.
#[template("examples/templates/minimal.html")]
#[proxy]
struct Minimal {
    #[default(String::new())]
    name: String,
}

impl Minimal {
    fn click(&self) -> bool {
        self.set_name("Привет, мир!".to_owned());
        true
    }
}

fn main() -> light_fw::Result<()> {
    light_fw::run(Minimal::new())
}
