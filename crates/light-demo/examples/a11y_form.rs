//! Пример доступной формы: заголовок `h1`, поля с `aria-label`, `required`
//! и `:validator`, флажок и регион статуса `role="status"` с
//! `aria-live="polite"`.
//!
//! Проверку выполняет рантайм при нажатии кнопки отправки: невалидные поля
//! подсвечиваются, получают `aria-description` и озвучиваются диктором. При
//! успехе меняется текст статуса, а соответствующий фрагмент дерева
//! доступности помечается как живой регион — скринридер объявит его сам.

use light::prelude::*;

/// Компонент из `examples/templates/a11y_form.html`.
#[template("examples/templates/a11y_form.html")]
#[proxy]
struct A11yForm {
    name: String,
    email: String,
    agree: bool,
    #[default("100%".to_owned())]
    title_width: String,
    #[default("#c9938f".to_owned())]
    accent: String,
    #[default("Заполните поля и отправьте форму.".to_owned())]
    message: String,
}

impl A11yForm {
    /// Кнопка отправки активна только при согласии с условиями.
    fn blocked(&self) -> bool {
        !self.agree()
    }

    /// Проверяет обязательные поля и пишет результат в живой регион.
    /// Проверка формата адреса (`:validator`). Пустое значение допустимо —
    /// его отсекает атрибут `required`.
    fn validate_email(&self, value: &str) -> Result<(), String> {
        if value.trim().is_empty() || value.contains('@') {
            Ok(())
        } else {
            Err("Адрес должен содержать @.".to_owned())
        }
    }

    /// Вызывается только после успешной проверки `required` и `:validator`.
    fn submit(&self) {
        self.set_message(format!("Спасибо, {}! Форма отправлена.", self.name()));
        self.set_accent("#2E7D32".to_owned());
    }
}

fn main() -> light::Result<()> {
    let app = A11yForm::new();
    if std::env::args().any(|arg| arg == "--dump-a11y") {
        let tree = app.render();
        let update = light::a11y::tree::build_update(&tree, None);
        println!(
            "{}",
            serde_json::to_string_pretty(&update).expect("сериализация дерева доступности")
        );
        return Ok(());
    }
    light::run(app)
}
