//! Trybuild: `#[template]` с неизвестным кастомным тегом → `compile_error!`.
//!
//! В текущем крейте (`light-fw-macros`) `[package.metadata.light.provides]`
//! отсутствует, поэтому любой нестандартный тег в HTML — ошибка. Тест
//! проверяет, что пользователь получает понятное сообщение с подсказкой
//! (Plugin API, ТЗ 0.19.0 Этап 4).
//!
//! Путь к шаблону — относительный от корня trybuild-проекта
//! (`target/tests/trybuild/light-fw-macros/`), где `CARGO_MANIFEST_DIR`
//! указывает на этот каталог при компиляции.

use light_fw::{proxy, template};

#[template("../../../../crates/light-macros/templates/ui_unknown_tag.html")]
#[proxy]
struct UiApp {
    title: String,
}

fn main() {}
