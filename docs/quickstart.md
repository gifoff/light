# Быстрый старт

Самый простой способ создать проект — установить CLI и выбрать шаблон:

```bash
cargo install light
light init my-app --template counter
cd my-app
light dev
```

Доступны четыре showcase-шаблона: `counter`, `todo`, `calculator` и `dashboard`.
Их исходники лежат в каталоге [`examples/`](https://github.com/gifoff/light/tree/master/examples).

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
edition = "2021"

[dependencies]
light-fw = { git = "https://github.com/gifoff/light" }

[features]
dev-mode = ["light-fw/dev-mode"]
```

`src/main.rs`:

```rust
use light_fw::prelude::*;

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
    // Использование run_with_config включает поддержку light.config.json
    // и автоматический запуск DevTools (если фича включена).
    light::run_with_config("light.config.json", App::new())
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

## Сборка и запуск

Light не изобретает свою build-систему — приложение собирается обычным `cargo`:

| Задача | Команда |
|---|---|
| Быстрый запуск (debug) | `cargo run` |
| Запуск с DevTools | `cargo run --features dev-mode` |
| Release-сборка | `cargo build --release` |
| Запуск release | `cargo run --release` |
| Hot-reload (HTML + `.rs`) | `light dev` |
| Hot-reload + сессия агента | `light dev --agent` |
| Сборка release + путь к бинарнику | `light build` |

**Разница между `cargo run` и `light dev`:**
- `cargo run` — обычная сборка и запуск. Правки в HTML **не подхватываются** (шаблон встроен в бинарник на этапе компиляции).
- `light dev` — запускает supervisor: собирает бинарник, следит за `templates/` и `src/`, при правках HTML шлёт `RELOAD_TEMPLATE` (мгновенно), при правках `.rs` — пересобирает и перезапускает с сохранением состояния.

**Когда что использовать:**
- Разработка: `light dev` (или `light dev --agent` для ИИ-агентов).
- CI / production: `cargo build --release`.
- Простой проект без hot-reload: `cargo run --features dev-mode` (DevTools работает, HTML — только через пересборку).

## Дальше

- [Руководство пользователя](guide.md) — директивы, стили, события и окна.
- [Архитектура](ARCHITECTURE.md) — как устроен фреймворк.
- [Разработка компонентов](COMPONENT_AUTHORING.md) — свои элементы и поведения.