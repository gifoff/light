//! Пример модального диалога доступности: `role="alertdialog"` со связями
//! `aria-labelledby`/`aria-describedby`, кнопки с собственными `aria-label`
//! и живой регион `role="status"` (`aria-live="assertive"`).
//!
//! Диалог построен на собственном окне с `id="a11y-dialog"`; закрыть его
//! можно крестиком в заголовке, а результат подтверждения объявляется
//! скринридером через живой регион.

use relay::prelude::*;

/// Компонент из `examples/templates/a11y_dialog.html`.
#[template("examples/templates/a11y_dialog.html")]
#[proxy]
struct A11yDialog {
    #[default("Действие ожидает подтверждения.".to_owned())]
    message: String,
}

impl A11yDialog {
    fn confirm(&self) {
        self.set_message("Элемент удалён.".to_owned());
    }

    fn cancel(&self) {
        self.set_message("Удаление отменено.".to_owned());
    }
}

fn main() -> relay::Result<()> {
    let app = A11yDialog::new();
    if std::env::args().any(|arg| arg == "--dump-a11y") {
        let tree = app.render();
        let update = relay::a11y::tree::build_update(&tree, None);
        println!(
            "{}",
            serde_json::to_string_pretty(&update).expect("сериализация дерева доступности")
        );
        return Ok(());
    }
    relay::run(app)
}
