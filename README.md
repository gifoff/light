# Light

[![CI](https://github.com/gifoff/light/actions/workflows/ci.yml/badge.svg)](https://github.com/gifoff/light/actions/workflows/ci.yml)
[![Docs](https://img.shields.io/badge/docs-gifoff.github.io%2Flight-blue)](https://gifoff.github.io/light/)
[![crates.io](https://img.shields.io/crates/v/light.svg)](https://crates.io/crates/light)

Light — декларативный реактивный GUI-фреймворк для Rust. Вы пишете HTML-разметку с директивами (`v-if`, `v-for`, `v-model`), описываете состояние реактивными свойствами (`#[proxy]`), а Light компилирует шаблоны **на этапе сборки** в Rust-код, строящий виртуальный DOM. 

Окна создаются на `winit`, а программно нарисованный кадр показывается через `softbuffer`; `wgpu` доступен отдельной фичей. Light даёт привычный DX (как в Vue.js), но с производительностью и надёжностью нативного Rust.

## Зачем использовать Light?

- **CLI для всего.** `light init`, `light dev`, `light build` — вам не нужно вручную настраивать сборку и зависимости.
- **Компиляция вместо интерпретации.** Шаблоны превращаются в Rust на этапе сборки. В рантайме нет HTML-парсера и интерпретатора разметки.
- **Гранулярная реактивность.** Изменение свойства перерисовывает только зависящие узлы виртуального дерева.
- **Нативные окна.** Безрамочные окна, кастомный titlebar, модальные окна и несколько окон — из коробки.
- **Платите только за используемое.** Каждая подсистема — Cargo-фича; код выключенных возможностей не попадает в бинарник.
- **Дружелюбен к ИИ-агентам.** Уникальный `light devtools` позволяет автономным агентам тестировать UI и собирать скриншоты.

## 🚀 Быстрый старт (3 шага)

Самый простой способ создать проект — установить CLI и запустить `init`:

```bash
# 1. Устанавливаем CLI
cargo install light

# 2. Создаём проект
light init my-app
cd my-app

# 3. Запускаем с hot-reload
light dev
```

Готово! Откроется окно вашего первого приложения.

Доступны готовые шаблоны showcase-приложений (`counter`, `todo`, `calculator`):
```bash
light init my-app --template counter
```

## Статус

Текущая версия — **0.17.6**. Проект в стадии **пре-1.0**: публичный API ещё не стабилен и может меняться между минорными версиями. Мы следуем семверу.

## Ручная сборка (без CLI)

Если вы предпочитаете классический `cargo` без использования CLI-утилиты, создайте проект вручную.

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
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

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
    // run_with_config включает поддержку light.config.json и DevTools
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

```bash
cargo run
# или cargo run --features dev-mode (для DevTools)
```
> **Примечание:** При использовании `cargo run` правки в HTML-файлах не подхватываются автоматически. Используйте `light dev` для горячей перезагрузки.

## Showcase

| Calculator | Todo | Dashboard |
| --- | --- | --- |
| ![Calculator](docs/assets/calculator.png) | ![Todo](docs/assets/todo.png) | ![Dashboard](docs/assets/dashboard.png) |

Полноценный dashboard с изображениями, вкладками и модальным окном:
```bash
cargo run -p example-dashboard
```

## Разработка и DevTools

Запустите `light dev` — Light поднимет supervisor, который следит за `templates/` и `src/`. 
- При изменении HTML мгновенно применяется `RELOAD_TEMPLATE` (без пересборки).
- При изменении `.rs` происходит инкрементальная пересборка с сохранением состояния окна.

Под капотом `light dev` также запускает TCP-сервер (порт `9229`), который отдаёт JSON-снимок состояния UI.

### Agent-friendly DevTools (0.17+)

Light можно назвать первым GUI-фреймворком для Rust, спроектированным с учётом ИИ-агентов. 
Команда `light devtools` даёт агенту «увидеть» UI (дерево VDOM, раскладка, снимок состояния, скриншот) и «потрогать» его (клик, ввод текста, клавиши) по стабильным ключам узлов.

```bash
light dev --agent                 # Запускает приложение + записывает сессию агента
light devtools tree               # JSON-дерево VDOM
light devtools find --tag button  # Найти элемент (ключ + координаты)
light devtools click --key .0.1   # Программный клик
light devtools shot --path shots  # Сделать скриншот (PNG)
```

Флаг `--agent` создаёт `.light/agent-sessions/<id>/` с журналом действий (`actions.jsonl`) и генерирует `summary.md` — готовый отчёт с доказательствами для агента.

## Управление компонентами (CLI)

Light поддерживает модульные компоненты (например, элементы форм или валидацию).

```bash
light add light-fw-element-form    # Добавить компонент в проект
light search datepicker            # Поиск компонентов на crates.io
```

Если вы используете компонент в шаблоне, но забыли его установить, `light dev` или `light build` автоматически обнаружат это и предложат установить (в неинтерактивном режиме — установят сами).

## Крейты воркспейса

Light разделён на крейты, чтобы вы подключали только то, что вам нужно:

| Крейт | Назначение |
| --- | --- |
| **`light`** | CLI-утилита (`cargo install light`): dev-сервер, сборка, менеджер компонентов. |
| **`light-fw`** | Мета-крейт (библиотека): реэкспортирует ядро, макросы и подключённые компоненты. |
| **`light-fw-core`** | Ядро: VDOM, реактивность, раскладка, отрисовка, окно. |
| **`light-fw-macros`** | Процедурные макросы `#[template]` и `#[proxy]`. |
| **`light-fw-html`** | Разбор HTML-шаблонов. |
| **`light-fw-element-form`** | Компонент-элемент: `<checkbox>`, `<radio>`, `<select>`. |
| **`light-fw-behavior-validation`** | Валидация: `required`, `:validator`, `@validate`. |
| **`light-fw-cli-core`** | Внутренняя логика CLI. |

## Документация

Сайт: [gifoff.github.io/light](https://gifoff.github.io/light/).

- [Быстрый старт](docs/quickstart.md) — установка и первый проект.
- [Руководство пользователя](docs/guide.md) — директивы, стили, события, окна, DevTools.
- [Справочник CLI](docs/cli.md) — все команды `light`.
- [Архитектура](docs/ARCHITECTURE.md) — устройство фреймворка для контрибьюторов.
- [Roadmap](docs/ROADMAP.md) — планы по развитию.

## Лицензия

Распространяется под лицензией `MIT OR Apache-2.0` на ваш выбор. Полные тексты — в файлах [LICENSE-MIT](LICENSE-MIT) и [LICENSE-APACHE](LICENSE-APACHE).