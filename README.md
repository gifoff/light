# Light

[![CI](https://github.com/gifoff/light/actions/workflows/ci.yml/badge.svg)](https://github.com/gifoff/light/actions/workflows/ci.yml)
[![Docs](https://img.shields.io/badge/docs-gifoff.github.io%2Flight-blue)](https://gifoff.github.io/light/)

Light — декларативный реактивный GUI-фреймворк для Rust. Вы пишете HTML-разметку
с директивами (`v-if`, `v-for`, `v-model`), описываете состояние реактивными
свойствами (`#[proxy]`), а Light компилирует шаблоны **на этапе сборки** в
Rust-код, строящий виртуальный DOM. Окна создаются на `winit`, а программно
нарисованный кадр показывается через `softbuffer`; `wgpu` доступен отдельной
фичей `wgpu-backend`.

- **Компиляция вместо интерпретации.** Шаблоны превращаются в Rust на этапе
  сборки: в рантайме нет HTML-парсера и интерпретатора разметки.
- **Гранулярная реактивность.** Изменение свойства перерисовывает только
  зависящие узлы виртуального дерева.
- **Нативные окна.** Безрамочные окна, кастомный titlebar, модальные окна и
  несколько окон — из коробки.
- **Изображения.** PNG встраиваются в бинарник на этапе компиляции:
  `<image src="…">` и иконка окна `icon="…"` (фича `image-support`).
- **Платите только за используемое.** Каждая подсистема — Cargo-фича; код
  выключенных возможностей не попадает в бинарник.
- **Компактные бинарники.** Программный рендеринг через `softbuffer` включён
  по умолчанию: release-сборка демо занимает ~1.7 MB вместо ~7.7 MB с `wgpu`.

## Статус

Текущая версия — **0.10.0**. Проект в стадии **пре-1.0**: публичный API ещё не
стабилен и может меняться между минорными версиями.

## Быстрый старт

Создать проект с готовыми компонентами и найти сторонние можно через CLI:

```bash
cargo install light-fw-cli
light init my-app --with form --with validation
light search datepicker
```

Вручную проект собирается так:

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

```bash
cargo run
```

### Создание проекта через CLI

Если установлена утилита `light`, скелет проекта создаётся одной командой:

```bash
cargo install light-fw-cli
light init my-app
```

Дополнительные компоненты подключаются флагом `--with` (можно повторять):

```bash
light init my-app --with form --with validation
```

`form` добавляет `light-fw-element-form` и фичу `light/elements-form`,
`validation` — `light-fw-behavior-validation` и фичу `light/validation`.
В `Cargo.toml` прописываются и зависимости, и нужные фичи мета-крейта.

### Готовые шаблоны

Вместо пустого окна можно взять showcase-приложение:

```bash
light init my-app --template counter
light init my-app --template todo
light init my-app --template calculator
```

Шаблоны и их HTML встроены в бинарник через `include_str!`, поэтому создание
проекта не требует сети. Исходники примеров лежат в `examples/`.

## Showcase

| Calculator | Todo | Dashboard |
| --- | --- | --- |
| ![Calculator](docs/assets/calculator.png) | ![Todo](docs/assets/todo.png) | ![Dashboard](docs/assets/dashboard.png) |

Полноценный dashboard с изображениями, вкладками и модальным окном:

```bash
cargo run -p example-dashboard
```

### Добавление компонентов

```bash
light search datepicker
light add light-fw-element-form
```

`light search` ищет компоненты на crates.io. `light add` разрешает компонент
во встроенном реестре, workspace или `vendor/`.
Если крейта там нет, он ищется на crates.io — это позволяет подключать
сторонние компоненты с префиксом `light-fw-`, например будущий
`light-fw-datepicker`. Фичи самого крейта передаются через `--features`:

```bash
light add light-fw-datepicker --features calendar,timezone
```

`light add` обновляет `Cargo.toml` и `light.lock`, а `light remove` удаляет
как встроенные, так и внешние компоненты.

## Крейты воркспейса

| Крейт | Назначение |
| --- | --- |
| `light` | Мета-крейт: реэкспортирует ядро, макросы и подключённые компоненты. |
| `light-fw-core` | Ядро: виртуальный DOM, реактивность, раскладка, отрисовка, окно. |
| `light-fw-macros` | Процедурные макросы `#[template]` и `#[proxy]`. |
| `light-fw-html` | Разбор HTML-шаблонов для макросов. |
| `light-fw-element-form` | Компонент-элемент: `<checkbox>`, `<radio>`, `<select>`/`<option>`. |
| `light-fw-behavior-validation` | Компонент-поведение: `required`, `:validator`, `@validate`. |
| `light-fw-cli` | Утилита `light`: проверка проекта и управление компонентами. |
| `light-fw-cli-core` | Ядро CLI: анализ проекта, `light.lock`, управление компонентами. |

## Производительность

Бенчмарки запускаются через [Criterion](https://github.com/bheisler/criterion.rs):

```bash
cargo bench -p light-fw-core --bench performance
```

Ориентировочные результаты на Intel Core i5-13500 (Windows, release-профиль):

| Сценарий | Время |
| --- | --- |
| VDOM diff, 1000 элементов, 10 изменений | ~101 µs |
| Раскладка flex-дерева из 1000 элементов | ~143 µs |
| Отрисовка 1000 прямоугольников в RGBA-буфер 800×600 | ~11.9 ms |
| Отрисовка 100 текстовых строк | ~9.4 ms |

Подробности и методика — в [docs/BENCHMARKS.md](docs/BENCHMARKS.md).

## Документация

Сайт: [gifoff.github.io/light](https://gifoff.github.io/light/).

Собрать документацию локально:

```bash
cargo install mdbook
mdbook serve
```

- [docs/SUMMARY.md](docs/SUMMARY.md) — оглавление книги.
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) — обзор архитектуры для
  контрибьюторов.
- [docs/guide.md](docs/guide.md) — руководство пользователя.
- [docs/MIGRATION_0.3_TO_0.4.md](docs/MIGRATION_0.3_TO_0.4.md) — миграция
  с 0.3 на 0.4 (softbuffer по умолчанию).
- [docs/COMPONENT_MODEL.md](docs/COMPONENT_MODEL.md) — модель компонентов.
- [docs/COMPONENT_AUTHORING.md](docs/COMPONENT_AUTHORING.md) — как писать свои
  компоненты.
- [docs/EXTERNAL_COMPONENTS.md](docs/EXTERNAL_COMPONENTS.md) — краткий гайд по
  созданию и публикации стороннего компонента.
- [docs/RELEASING.md](docs/RELEASING.md) — публикация релиза (для мейнтейнеров).
- [docs/BENCHMARKS.md](docs/BENCHMARKS.md) — бенчмарки и их результаты.
- [docs/ROADMAP.md](docs/ROADMAP.md) — план развития.

## Ссылки
- Основной репозиторий: [GitHub](https://github.com/gifoff/light)
- Зеркало: [GitVerse](https://gitverse.ru/gifoff/light)

## Лицензия

Распространяется под лицензией `MIT OR Apache-2.0` на ваш выбор. Полные тексты —
в файлах [LICENSE-MIT](LICENSE-MIT) и [LICENSE-APACHE](LICENSE-APACHE).
