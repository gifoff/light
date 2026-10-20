# Написание компонентов Light

Практическое руководство по созданию собственных компонентов Light: от
пустого крейта до публикации в crates.io. Архитектуру компонентной модели
(что такое компонент, типы, распространение) описывает
[docs/COMPONENT_MODEL.md](COMPONENT_MODEL.md); здесь — конкретные шаги,
манифест и примеры кода.

Документ адресован **сторонним разработчикам**: тем, кто хочет выпустить
свой виджет, поведение или интеграцию и подключить их к приложению на Light.

- [1. Введение](#1-введение)
- [2. Типы компонентов и их назначение](#2-типы-компонентов-и-их-назначение)
- [3. Шаг 1: Создание проекта](#3-шаг-1-создание-проекта)
- [4. Шаг 2: Манифест `Cargo.toml`](#4-шаг-2-манифест-cargotoml)
- [5. Шаг 3: Реализация компонента](#5-шаг-3-реализация-компонента)
- [6. Шаг 4: Тестирование](#6-шаг-4-тестирование)
- [7. Шаг 5: Подготовка к публикации](#7-шаг-5-подготовка-к-публикации)
- [8. Лучшие практики](#8-лучшие-практики)
- [9. Ограничения (что нельзя сейчас)](#9-ограничения-что-нельзя-сейчас)
- [10. Что дальше](#10-что-дальше)
- [11. Ссылки](#11-ссылки)

---

## 1. Введение

**Компонент Light** — это автономный Rust-крейт (или пакет данных), который
добавляет фреймворку функциональность: новые элементы разметки, поведение
существующих элементов, готовые составные виджеты, интеграции с ОС или целые
подсистемы. Компонент описывает себя в `Cargo.toml` секцией
`[package.metadata.light]`, объявляет совместимость с ядром `light-fw-core` и
подключается к приложению обычной Cargo-зависимостью. Включение происходит на
этапе сборки: код неиспользуемого компонента не попадает в бинарник
(см. [docs/DESIGN_PRINCIPLES.md](DESIGN_PRINCIPLES.md)).

Сегодня уже можно создавать **виджеты**, **поведения** и **интеграции**,
собранные из публичного API ядра и существующих элементов. Создать
**принципиально новый примитивный DOM-тег** (`NodeKind`) без доработки
`light-fw-core` пока нельзя — это описано в разделе
[«Ограничения»](#9-ограничения-что-нельзя-сейчас).

## 2. Типы компонентов и их назначение

Тип компонента задаётся полем `kind` в манифесте. Таксономия совпадает с
[docs/COMPONENT_MODEL.md](COMPONENT_MODEL.md), здесь — практический фокус.

| `kind`        | Назначение                                              | Что уже есть                        | Что могло бы быть              |
| ------------- | ------------------------------------------------------- | ----------------------------------- | ------------------------------ |
| `element`     | Новые базовые элементы разметки (теги)                  | `light-fw-element-form`                | `light-element-media`          |
| `behavior`    | Расширение существующих элементов (атрибуты, директивы) | `light-fw-behavior-validation`         | `light-behavior-drag`          |
| `widget`      | Составные компоненты из elements + behaviors            | —                                   | `light-widget-datepicker`      |
| `integration` | Мост с ОС и внешними сервисами (Rust-API, не теги)      | —                                   | `light-integration-clipboard`  |
| `subsystem`   | Крупные подсистемы фреймворка                           | —                                   | `light-subsystem-i18n`         |
| `dev`         | Только для разработки, вне релизной сборки              | —                                   | `light-devtools`               |

Тип — это не формальность: он определяет, что компонент имеет право
`предоставлять` (`provides`). Например, `behavior` не может вводить новые
теги, а `element` — регистрировать директивы.

> **Сегодня на практике.** `element`-компоненты (`light-fw-element-form`)
> включают уже реализованные в ядре элементы через Cargo-фичи; по-настоящему
> *новые* теги требуют изменения `light-fw-core`. Поэтому для стороннего
> разработчика наиболее доступны `behavior`, `widget` и `integration`,
> которые не расширяют ядро, а компонуют существующее.

## 3. Шаг 1: Создание проекта

Создайте библиотечный крейт:

```powershell
cargo new light-widget-my-widget --lib
cd light-widget-my-widget
```

Структура проекта:

```
light-widget-my-widget/
├── Cargo.toml
├── README.md
├── LICENSE-MIT
├── LICENSE-APACHE
├── templates/            # если виджет использует HTML-шаблоны
│   └── my_widget.html
└── src/
    └── lib.rs
```

Совет: если компонент один — **не создавайте отдельный workspace**.
`cargo new` создаёт обычный крейт, который можно разрабатывать и публиковать
самостоятельно. Workspace нужен только когда компонентов несколько и вы
хотите делить версии.

## 4. Шаг 2: Манифест `Cargo.toml`

Это главный раздел: здесь фиксируется контракт компонента. Полный пример для
виджета:

```toml
[package]
name = "light-widget-datepicker"
version = "0.1.0"
edition = "2024"
# Минимальная версия Rust должна покрывать требования `light-fw-core`
# (сейчас — 1.95) и редакцию 2024.
rust-version = "1.95"
license = "MIT OR Apache-2.0"
description = "Виджет выбора даты для Light"
repository = "https://github.com/<user>/light-widget-datepicker"
keywords = ["light", "widget", "date", "input"]
categories = ["gui"]

[dependencies]
# Крейт `light` реэкспортирует ядро и макросы `#[template]`/`#[proxy]`.
# Пока версии не опубликованы в crates.io, путь указывает на локальную
# копию (соседний checkout или каталог `vendor/`). После публикации
# (0.4) достаточно строки версии: light = "0.4".
light = { path = "../light/crates/light", default-features = false, features = [
    "elements-basic",
    "elements-form",
] }
# Если виджет использует элементы формы — подключите соответствующий компонент:
light-fw-element-form = { path = "../light/crates/light-element-form" }

[package.metadata.light]
kind = "widget"
display-name = "Date Picker"
core = "^0.4"

[package.metadata.light.provides]
elements = ["datepicker"]     # новые теги, которые предоставляет компонент
attributes = []               # новые атрибуты (для behavior)
directives = []               # новые директивы (для behavior)

[package.metadata.light.requires]
components = [
    "light-fw-element-form ^0.4",
]

[package.metadata.light.registry]
official = false
tags = ["form", "date", "input", "calendar"]
```

### Поля манифеста

- **`kind`** — тип компонента из таксономии: `element`, `behavior`,
  `widget`, `integration`, `subsystem`, `dev`.
- **`display-name`** — человекочитаемое имя для CLI, документации и каталога.
- **`core`** — **semver-range** совместимости с ядром, не точная версия.
  `^0.4` означает «совместим с `0.4.x`, но не с `0.5`». Обновляйте значение,
  когда ядро выпускает новую несовместимую версию.
- **`provides.elements`** — новые теги, которые компонент добавляет в
  разметку. По ним CLI сопоставляет использование элементов и компонентов.
- **`provides.attributes`** — новые атрибуты (например, `:validator`).
- **`provides.directives`** — новые директивы (например, `@validate`).
- **`requires.components`** — обязательные компоненты в формате
  `"<имя> <semver-range>"`. Зависимости всегда явные: никаких «неявных»
  требований (см. `COMPONENT_MODEL.md`, §2).
- **`conflicts`** *(необязательно)* — список несовместимых компонентов:
  их нельзя включать одновременно.
- **`registry.official`** / **`registry.tags`** *(необязательно)* —
  метаданные для каталога и поиска.

### Требования к зависимостям и макросам

Макросы `#[template]` и `#[proxy]` разворачиваются в пути `::light::…`,
поэтому **зависимость обязана называться `light`**. Компонент, который
определяет шаблоны и реактивные свойства, должен зависеть от мета-крейта
`light` (он реэкспортирует ядро и макросы). Компонент без макросов — например,
`light-fw-behavior-validation`, только реэкспортирующий функции ядра — может
зависеть непосредственно от `light-fw-core`.

### Что проверяет CLI

- **`light add` и `light doctor`** — при разрешении компонента читают его
  `[package.metadata.light]`: поле `kind` должно быть одним из допустимых, а
  `core` — корректным semver-range (иначе возвращается ошибка разбора
  манифеста).
- **`light check`** — работает с **проектом-потребителем**: сканирует его
  `*.html`-шаблоны и сопоставляет теги/атрибуты/директивы со **встроенным**
  реестром (`crates/light-cli-core/registry.toml`).
- **`light publish`** (появится в 0.3) — будет проверять манифест целиком,
  включая `repository` (обязателен для публикации) и согласованность
  `provides`/`requires`.

### Чего сейчас нет

- **Новых `NodeKind`** без изменения `light-fw-core`. До появления plugin-API
  виджеты собираются из *существующих* элементов (`<div>`, `<button>`,
  `<text>` и т. д.) и композиции.
- **Своего реестра**: `light check` знает только компоненты из встроенного
  `registry.toml`. Чтобы сторонний компонент распознавался, его описание
  нужно добавить в реестр (см. [«Ограничения»](#9-ограничения-что-нельзя-сейчас)).

## 5. Шаг 3: Реализация компонента

### Виджет на шаблоне и свойствах (идиоматично)

Виджет объявляет структуру с реактивными свойствами (`#[proxy]`) и привязывает
её к HTML-шаблону (`#[template]`). Шаблон компилируется на этапе сборки в
`impl Component for DatePicker`.

`templates/datepicker.html`:

```html
<div direction="vertical">
    <text v-model="value" placeholder="YYYY-MM-DD" />
    <button @click="toggle_calendar">Календарь</button>
</div>
```

`src/lib.rs`:

```rust
use light::prelude::*;

#[template("templates/datepicker.html")]
#[proxy]
pub struct DatePicker {
    value: String,
    is_open: bool,
}

impl DatePicker {
    fn toggle_calendar(&self) {
        self.set_is_open(!self.is_open());
    }
}
```

`#[proxy]` для каждого поля генерирует геттер (`value()`) и сеттер
(`set_value(...)`); изменение свойства перерисовывает только зависящие узлы.

### Минимальный программный узел

Когда нужен узел без шаблона — например, для unit-тестов — соберите
`VElement` напрямую. Публичные типы и конструкторы даёт `light::prelude`:

```rust
use light::prelude::*;
use light::Resize;

#[derive(Debug, Default, Clone)]
pub struct DatePicker {
    value: String,
    is_open: bool,
}

impl DatePicker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn value(&self) -> &str {
        &self.value
    }

    pub fn render(&self) -> VElement {
        VElement::container(
            "",
            NodeKind::Column,
            vec![
                VElement::leaf("field", NodeKind::TextField {
                    value: self.value.clone(),
                    placeholder: "YYYY-MM-DD".to_string(),
                    disabled: false,
                    multiline: false,
                    resize: Resize::None,
                    masked: false,
                })
                .with_tag("text"),
            ],
        )
    }
}
```

Публичные конструкторы `VElement`: `leaf`, `container`, `with_tag`,
`with_attr`, `with_attrs`, `with_style`, `with_window`. Тип узла задаёт
`NodeKind`; вариант `NodeKind::TextField` доступен всегда, а `Checkbox`,
`Radio`, `Select` — только при включённых фичах `elements-checkbox`,
`elements-radio`, `elements-select` (их включает `light-fw-element-form`).

### Поведение

Поведение не создаёт элементов, а добавляет функциональность существующим —
через атрибуты и директивы. Например, валидация: метод компонента с сигнатурой
`fn(&self, value: &str) -> Result<(), String>` подключается атрибутом
`:validator`.

```rust
use light::prelude::*;

#[proxy]
struct SignupForm {
    email: String,
}

impl SignupForm {
    fn validate_email(&self, value: &str) -> Result<(), String> {
        if value.contains('@') {
            Ok(())
        } else {
            Err("Укажите корректный e-mail".to_string())
        }
    }
}
```

Использование в шаблоне:

```html
<div role="form" aria-label="Регистрация">
    <text v-model="email" required :validator="validate_email" />
    <button type="submit">Отправить</button>
</div>
```

> **Ограничение (честно).** Пока нет официального API для «регистрации нового
> DOM-элемента» из компонента. Новые теги и новые варианты `NodeKind`
> добавляются только изменением `light-fw-core` (или ожиданием plugin-API).
> Сторонний `behavior`/`widget` работает поверх публичного API и композиции
> существующих элементов.

## 6. Шаг 4: Тестирование

### Unit-тесты

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn datepicker_starts_empty() {
        let dp = DatePicker::new();
        assert_eq!(dp.value(), "");
        assert!(!dp.is_open());
    }
}
```

### Интеграционные тесты с реальным `VElement`

```rust
#[test]
fn datepicker_renders() {
    let element = DatePicker::new().render();
    assert!(matches!(element.kind, NodeKind::Column));
    assert_eq!(element.children.len(), 1);
}
```

### Проверка через CLI

В проекте-потребителе (или в самом компоненте, если в нём есть HTML-шаблоны
и он является приложением) запустите:

```powershell
light check --strict
```

`light check` не запускает сборку и не пишет на диск: он сканирует
`*.html`-шаблоны и сверяет их со встроенным реестром. Для самого
компонента-библиотеки код возврата `0` означает, что в шаблонах нет ошибок
разбора. Команда не проверяет `[package.metadata.light]` вашего крейта — это
сделает `light publish` (0.3) или `light doctor` при подключении компонента к
проекту.

## 7. Шаг 5: Подготовка к публикации

### Чек-лист перед публикацией

- [ ] `license = "MIT OR Apache-2.0"` в `Cargo.toml`.
- [ ] Файлы `LICENSE-MIT` и `LICENSE-APACHE` в корне.
- [ ] `repository` в `Cargo.toml` (обязателен для публикации).
- [ ] `description` в `Cargo.toml`.
- [ ] `keywords` (до 5) и `categories`.
- [ ] `README.md` с описанием и примером.
- [ ] Заполнена секция `[package.metadata.light]`.
- [ ] `cargo clippy --all-targets -- -D warnings` — без предупреждений.
- [ ] `cargo test` — зелёный.
- [ ] `cargo doc --no-deps` — без предупреждений.
- [ ] Версия по semver: первая публикация — `0.1.0`.

### Публикация

После реализации команды `light publish` (0.3):

```powershell
light publish
```

До этого момента публикуйте вручную через Cargo:

```powershell
cargo publish
```

> Сейчас `light` и `light-fw-core` ещё не опубликованы в crates.io, поэтому
> «публикация компонента» возможна только после их выхода (0.3). До этого
> компонент используется локально: как член workspace или через `light vendor`
> (копирование исходников в `vendor/` проекта-потребителя).

## 8. Лучшие практики

**Именование.** Префикс отражает тип:

- `light-element-<name>` — элемент;
- `light-behavior-<name>` — поведение;
- `light-widget-<name>` — виджет;
- `light-integration-<name>` — интеграция;
- `light-subsystem-<name>` — подсистема.

**Версионирование.** Следуйте semver: ломающие изменения — major (или minor
для версий до `1.0`). Поле `core` — диапазон совместимости с ядром; обновляйте
его, когда `light-fw-core` выпускает новую несовместимую версию.

**Зависимости.** Минимизируйте их. Используйте `default-features = false` и
включайте только нужные фичи. Избегайте `unsafe` без обоснования.

**Документация.** Rustdoc на всём публичном API; README с примером.

**Тестирование.** Покрывайте публичный API и граничные случаи.

**Реактивность.** Помните: `#[proxy]` даёт геттеры/сеттеры, а `#[template]`
компилируется на этапе сборки — ошибки разметки видны компилятору с указанием
файла и строки.

## 9. Ограничения (что нельзя сейчас)

- **Новые `NodeKind` и примитивные теги.** Только через изменение
  `light-fw-core`. Plugin-API для регистрации новых элементов появится в 0.3+.
- **Изменение `light-fw-core` из компонента.** Только через публичный API;
  вмешательство во внутренние слои не поддерживается.
- **Asset packs.** Темы, иконки и локали — отдельная подсистема (0.3+), не
  часть кода компонента.
- **Приватные реестры.** Пока только crates.io.
- **Распознавание в `light check`.** Команда использует встроенный
  `registry.toml`; сторонний компонент станет виден только после добавления
  его описания в реестр.

## 10. Что дальше

- **0.3** — команда `light publish` и публикация `light-fw-core`/`light` в
  crates.io.
- **0.3+** — plugin-API для новых элементов (`NodeKind`) без изменения ядра.
- **0.4+** — asset packs: темы, иконки, локали.

## 11. Ссылки

- [docs/COMPONENT_MODEL.md](COMPONENT_MODEL.md) — архитектура компонентов.
- [docs/guide.md](guide.md) — руководство пользователя.
- [docs/ROADMAP.md](ROADMAP.md) — план развития.
- [docs/DESIGN_PRINCIPLES.md](DESIGN_PRINCIPLES.md) — принципы, включая
  «только используемый код попадает в бинарник».
- Примеры в репозитории:
  - `crates/light-element-form/` — компонент-элемент;
  - `crates/light-behavior-validation/` — компонент-поведение.
- Поиск опубликованных компонентов (после 0.3): `cargo search light-`.
