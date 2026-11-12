# Быстрый старт

Самый простой способ создать проект — установить CLI и выбрать шаблон:

```bash
cargo install light
light init my-app --template counter
cd my-app
light dev            # разработка с hot-reload (DevTools на 127.0.0.1:9229)
light build          # release-сборка + путь к .exe
```

Для пустого проекта без шаблона: `light init my-app`. Компоненты подключаются
флагом `--with`: `light init my-app --with form --with validation`.

Шаблон `--template` поддерживает три showcase-приложения: `counter`, `todo` и
`calculator`. Их исходники лежат в каталоге
[`examples/`](https://github.com/gifoff/light/tree/master/examples) — там же есть
`dashboard`, `devtools-demo`, `multi-window` и `async-counter` (запускаются через
`light dev` или `cargo run -p example-…`).

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
light-fw = "0.17"

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

fn main() -> light_fw::Result<()> {
    // Использование run_with_config включает поддержку light.config.json
    // и автоматический запуск DevTools (если фича включена).
    light_fw::run_with_config("light.config.json", App::new())
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

Запуск (основной — CLI; ручной `cargo` как альтернатива — в
[разделе ниже](#сборка-и-запуск)):

```bash
light dev            # hot-reload + DevTools
# light build        # release-сборка
# cargo run          # альтернатива без CLI (правки HTML не подхватываются)
```

## Сборка и запуск

Основной способ разработки и сборки — CLI Light (`light dev` и `light build`):
он сам вызывает нужные команды `cargo`, следит за файлами, сохраняет состояние
окна и выводит путь к собранному бинарнику. Ручной `cargo` подходит как
альтернатива для тех, кто предпочитает собирать вручную.

### Рекомендуемый: `light dev` / `light build`

| Задача | Команда |
|---|---|
| Разработка (hot-reload HTML + `.rs`, DevTools) | `light dev` |
| Разработка + сессия ИИ-агента | `light dev --agent` |
| Сборка release + путь и размер бинарника | `light build` |

```bash
light dev          # hot-reload; правки HTML применяются мгновенно
light build        # cargo build --release, затем показывает путь к .exe
```

- `light dev` — supervisor: собирает бинарник, следит за `templates/` и `src/`;
  правки HTML шлют `RELOAD_TEMPLATE` (мгновенно), правки `.rs` — инкрементальная
  пересборка и перезапуск с сохранением состояния.
- `light build` — оборачивает `cargo build --release` и после сборки печатает
  путь к бинарнику и его размер. Флаги: `-f form` (фича), `--no-release`
  (debug-сборка), `--no-auto-install`, дополнительные аргументы `cargo` после `--`.

### Ручной `cargo` (альтернатива)

Если вы предпочитаете собирать сами, обычный `cargo` тоже работает:

| Задача | Команда |
|---|---|
| Быстрый запуск (debug) | `cargo run` |
| Запуск с DevTools | `cargo run --features dev-mode` |
| Release-сборка | `cargo build --release` |
| Запуск release | `cargo run --release` |

Важная разница: при `cargo run` правки в HTML **не подхватываются** (шаблон
встроен в бинарник на этапе компиляции через `include_str!`), поэтому для
разработки предпочтительнее `light dev`.

### Как скрыть консольное окно в релизе?

По умолчанию Windows-приложение открывает консольное окно (CMD) вместе с GUI.
Чтобы в release-сборке консоль **не появлялась**, добавьте первую строкой в
`src/main.rs`:

```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
```

Атрибут применяется только в release (`not(debug_assertions)`): в debug-сборке
консоль остаётся — удобно для логов. С версии 0.17.6 ядро Light больше не
пишет отладочные строки (`light: cold start …`, `light: loaded fallback font …`)
в release-сборке — они видны только в debug, так что чистая консоль не нужна.
Все шаблоны `light init` и примеры из `examples/` уже содержат эту строку.

### Как добавить иконку файлу .exe (в Проводнике)?

Атрибут `<window icon="…">` меняет иконку окна и в трее, но **не** иконку файла
`.exe` в Проводнике. Чтобы у самого файла появилась иконка, нужен ресурс Windows
через `winres` + `build.rs`. Необязательный шаг:

1. Положите `app.ico` (32×32 и крупнее) рядом с `src/`.
2. В `[build-dependencies]` манифеста добавьте `winres = "0.1"`.
3. Создайте `build.rs`:

```rust
fn main() {
    if cfg!(target_os = "windows") {
        let mut res = winres::WindowsResource::new();
        res.set_icon("app.ico");
        res.compile().expect("не удалось скомпилировать ресурсы");
    }
}
```

Пересоберите (`light build`) — иконка появится у `.exe`. Для иконки *окна*
используйте `icon` у `<window>` (см. [guide.md](guide.md#окно)).

## Дальше

- [Руководство пользователя](guide.md) — директивы, стили, события и окна.
- [Архитектура](ARCHITECTURE.md) — как устроен фреймворк.
- [Разработка компонентов](COMPONENT_AUTHORING.md) — свои элементы и поведения.