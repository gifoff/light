//! Минимальный пример: только базовые элементы. Служит эталоном для замера
//! размера бинарника при разных наборах фич (см. `scripts/bloat.ps1`).

use relay::prelude::*;

/// Компонент из `examples/templates/minimal.html`.
#[template("examples/templates/minimal.html")]
#[proxy]
struct Minimal {
    #[default(String::new())]
    name: String,
}

fn main() -> relay::Result<()> {
    relay::run(Minimal::new())
}
