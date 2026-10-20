# Быстрый старт

Самый простой способ создать проект — установить CLI и выбрать шаблон:

```bash
cargo install light-fw-cli
light init my-app --template counter
cd my-app
cargo run
```

Доступны три showcase-шаблона: `counter`, `todo` и `calculator`. Их исходники
лежат в каталоге [`examples/`](https://github.com/gifoff/light/tree/master/examples).

Для пустого проекта без шаблона:

```bash
light init my-app
```

Дополнительные компоненты подключаются флагом `--with`:

```bash
light init my-app --with form --with validation
```

## Вручную

Если CLI не нужен, создайте проект обычным способом.

`Cargo.toml`:

```toml
[package]
name = "my-app"
version = "0.1.0"
edition = "2024"

[dependencies]
light = { git = "https://github.com/gifoff/light" }
```

`src/main.rs`:

```rust
use light::prelude::*;

#[template("templates/app.html")]
#[proxy]
struct App {
    #[default("Мир".to_owned())]
    name: String,
}

impl App {
    fn greet(&self) {
        self.set_name(format!("Привет, {}!", self.name()));
    }
}

fn main() -> light::Result<()> {
    light::run(App::new())
}
```

`templates/app.html`:

```html
<window title="My App" width="480" height="320">
    <div direction="vertical" padding="16">
        <h1>Привет, {{ name }}!</h1>
        <button @click="greet">Поздороваться</button>
    </div>
</window>
```

Запуск:

```bash
cargo run
```

## Дальше

- [Руководство пользователя](guide.md) — директивы, стили, события и окна.
- [Архитектура](ARCHITECTURE.md) — как устроен фреймворк.
- [Разработка компонентов](COMPONENT_AUTHORING.md) — свои элементы и поведения.
