//! Демонстрация библиотеки Relay: реактивные формы (флажок, переключатель,
//! выпадающий список, поле пароля), модальное дочернее окно, второе главное
//! окно с общим реактивным состоянием и немодальное дочернее окно, открываемое
//! по идентификатору через `open("settings")`.

use std::cell::OnceCell;

use relay::prelude::*;

/// Реактивное состояние, разделяемое между окнами приложения.
#[proxy]
struct Shared {
    count: i64,
    text: String,
}

thread_local! {
    /// Единственный экземпляр общего состояния для всех окон.
    static SHARED: OnceCell<Shared> = const { OnceCell::new() };
}

/// Возвращает клон общего состояния: клоны используют одни и те же ячейки,
/// поэтому изменение из любого окна перерисовывает все окна.
fn shared() -> Shared {
    SHARED.with(|cell| cell.get_or_init(Shared::new).clone())
}

/// Отдельный реактивный класс, вложенный в компонент.
#[proxy]
struct Counter {
    count: i64,
}

impl Counter {
    /// Кнопка «Сброс» блокируется, когда счётчик уже равен нулю.
    fn is_zero(&self) -> bool {
        self.count() == 0
    }
}

/// Компонент дочернего модального окна (`templates/dialog.html`).
#[template("templates/dialog.html")]
#[proxy]
struct Dialog {
    #[default(false)]
    done: bool,
    #[default(shared())]
    shared: Shared,
}

impl Dialog {
    fn bump_shared(&self) {
        let shared = self.shared();
        shared.set_count(shared.count() + 1);
    }
}

/// Компонент второго главного окна (`templates/inspector.html`).
#[template("templates/inspector.html")]
#[proxy]
struct Inspector {
    #[default(shared())]
    shared: Shared,
}

impl Inspector {
    fn bump_shared(&self) {
        let shared = self.shared();
        shared.set_count(shared.count() + 1);
    }
}

/// Компонент немодального дочернего окна (`templates/settings.html`),
/// открываемого по идентификатору: `@click="open('settings')"`.
#[template("templates/settings.html")]
#[proxy]
struct Settings {
    #[default("light".to_owned())]
    theme: String,
    #[default(shared())]
    shared: Shared,
}

/// Корневой компонент: шаблон `templates/main.html` включает секции
/// `templates/sections/{header,counter,greeting,form,cross}.html`.
///
/// `windows = [...]` регистрирует окна, доступные для открытия по `id`.
#[template(
    "templates/main.html",
    modal = Dialog,
    window = Inspector,
    windows = [Dialog, Inspector, Settings]
)]
#[proxy]
struct App {
    #[default("Relay Demo".to_owned())]
    title: String,
    #[default("Мир".to_owned())]
    name: String,
    #[default(true)]
    show_greeting: bool,
    #[default("free".to_owned())]
    plan: String,
    #[default("msk".to_owned())]
    city: String,
    agreed: bool,
    secret: String,
    note: String,
    counter: Counter,
    #[default(shared())]
    shared: Shared,
}

impl App {
    fn increment(&self) {
        let counter = self.counter();
        counter.set_count(counter.count() + 1);
    }

    fn reset(&self) {
        self.counter().set_count(0);
    }

    fn toggle_greeting(&self) {
        self.set_show_greeting(!self.show_greeting());
    }

    fn bump_shared(&self) {
        let shared = self.shared();
        shared.set_count(shared.count() + 1);
    }
}

fn main() -> relay::Result<()> {
    let app = App::new();

    // Обозреватель перезапускается при изменении счётчика.
    let counter = app.counter();
    let _counter_watcher = watch(move || println!("обозреватель: счётчик = {}", counter.count()));

    relay::run(app)
}
