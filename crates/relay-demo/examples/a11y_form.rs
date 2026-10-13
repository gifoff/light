//! Пример доступной формы: заголовок `h1`, поля с `aria-label` и `required`,
//! флажок и регион статуса `role="status"` с `aria-live="polite"`.
//!
//! После отправки текст статуса меняется, а соответствующий фрагмент дерева
//! доступности помечается как живой регион — скринридер объявит его сам.

use relay::prelude::*;

/// Компонент из `examples/templates/a11y_form.html`.
#[template("examples/templates/a11y_form.html")]
#[proxy]
struct A11yForm {
    name: String,
    email: String,
    agree: bool,
    #[default("Заполните поля и отправьте форму.".to_owned())]
    message: String,
}

impl A11yForm {
    /// Кнопка отправки активна только при согласии с условиями.
    fn blocked(&self) -> bool {
        !self.agree()
    }

    /// Проверяет обязательные поля и пишет результат в живой регион.
    fn submit(&self) {
        if self.name().trim().is_empty() {
            self.set_message("Введите имя.".to_owned());
        } else if self.email().trim().is_empty() {
            self.set_message("Введите адрес электронной почты.".to_owned());
        } else {
            self.set_message(format!("Спасибо, {}! Форма отправлена.", self.name()));
        }
    }
}

fn main() -> relay::Result<()> {
    let app = A11yForm::new();
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
