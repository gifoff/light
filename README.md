# Relay

Relay — декларативный реактивный GUI-фреймворк для Rust. Вы пишете HTML-разметку
с директивами (`v-if`, `v-for`, `v-model`), описываете состояние реактивными
свойствами (`#[proxy]`), а Relay компилирует шаблоны **на этапе сборки** в
Rust-код, строящий виртуальный DOM. Окна создаются на `winit`, а программно
нарисованный кадр показывается через `softbuffer`; `wgpu` доступен отдельной
фичей `wgpu-backend`.

- **Компиляция вместо интерпретации.** Шаблоны превращаются в Rust на этапе
  сборки: в рантайме нет HTML-парсера и интерпретатора разметки.
- **Гранулярная реактивность.** Изменение свойства перерисовывает только
  зависящие узлы виртуального дерева.
- **Нативные окна.** Безрамочные окна, кастомный titlebar, модальные окна и
  несколько окон — из коробки.
- **Платите только за используемое.** Каждая подсистема — Cargo-фича; код
  выключенных возможностей не попадает в бинарник.
- **Компактные бинарники.** Программный рендеринг через `softbuffer` включён
  по умолчанию: release-сборка демо занимает ~1.7 MB вместо ~7.7 MB с `wgpu`.

## Статус

Текущая версия — **0.4.0**. Проект в стадии **пре-1.0**: публичный API ещё не
стабилен и может меняться между минорными версиями.

## Быстрый старт

`Cargo.toml`:

```toml
[package]
name = "my-app"
version = "0.1.0"
edition = "2024"

[dependencies]
relay = { git = "https://github.com/relay-rs/relay" }
```

`src/main.rs`:

```rust
use relay::prelude::*;

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

fn main() -> relay::Result<()> {
    relay::run(App::new())
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

```bash
cargo run
```

## Крейты воркспейса

| Крейт                       | Назначение                                                         |
| --------------------------- | ------------------------------------------------------------------ |
| `relay`                     | Мета-крейт: реэкспортирует ядро, макросы и подключённые компоненты. |
| `relay-core`                | Ядро: виртуальный DOM, реактивность, раскладка, отрисовка, окно.    |
| `relay-macros`              | Процедурные макросы `#[template]` и `#[proxy]`.                     |
| `relay-html`                | Разбор HTML-шаблонов для макросов.                                  |
| `relay-element-form`        | Компонент-элемент: `<checkbox>`, `<radio>`, `<select>`/`<option>`.  |
| `relay-behavior-validation` | Компонент-поведение: `required`, `:validator`, `@validate`.         |
| `relay-framework-cli`       | Утилита `relay`: проверка проекта и управление компонентами.        |
| `relay-cli-core`            | Ядро CLI: анализ проекта, `relay.lock`, управление компонентами.    |

## Документация

- [docs/guide.md](docs/guide.md) — руководство пользователя.
- [docs/MIGRATION_0.3_TO_0.4.md](docs/MIGRATION_0.3_TO_0.4.md) — миграция
  с 0.3 на 0.4 (softbuffer по умолчанию).
- [docs/COMPONENT_MODEL.md](docs/COMPONENT_MODEL.md) — модель компонентов.
- [docs/COMPONENT_AUTHORING.md](docs/COMPONENT_AUTHORING.md) — как писать свои
  компоненты.
- [docs/RELEASING.md](docs/RELEASING.md) — публикация релиза (для мейнтейнеров).
- [docs/ROADMAP.md](docs/ROADMAP.md) — план развития.

## Лицензия

Распространяется под лицензией `MIT OR Apache-2.0` на ваш выбор. Полные тексты —
в файлах [LICENSE-MIT](LICENSE-MIT) и [LICENSE-APACHE](LICENSE-APACHE).
