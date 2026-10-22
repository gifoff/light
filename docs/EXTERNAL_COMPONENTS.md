# Сторонние компоненты Light

Короткий практический гайд: как создать, опубликовать и подключить собственный
компонент Light на примере `light-fw-datepicker`.

Более подробно об архитектуре и типах компонентов — в
[docs/COMPONENT_AUTHORING.md](COMPONENT_AUTHORING.md) и
[docs/COMPONENT_MODEL.md](COMPONENT_MODEL.md).

## 1. Создание крейта

Имя компонента должно начинаться с `light-fw-`: по этому префиксу `light add`
и `light search` отличают компоненты Light от обычных крейтов.

```bash
cargo new light-fw-datepicker --lib
cd light-fw-datepicker
```

## 2. Зависимость от Light

Добавьте мета-крейт `light` в `Cargo.toml`:

```toml
[package]
name = "light-fw-datepicker"
version = "0.1.0"
edition = "2024"
rust-version = "1.95"
license = "MIT OR Apache-2.0"
description = "Date picker component for Light"
repository = "https://github.com/<user>/light-fw-datepicker"

[dependencies]
light = "0.8.0"
```

Если компонент использует элементы формы, подключите
`light-fw-element-form` и фичу `light/elements-form`:

```toml
[dependencies]
light = { version = "0.8.0", features = ["elements-form"] }
light-fw-element-form = "0.8.0"
```

## 3. Минимальный компонент

Создайте `templates/datepicker.html`:

```html
<window title="Date picker" width="320" height="200">
    <div direction="vertical" padding="12">
        <text v-model="value" placeholder="YYYY-MM-DD" aria-label="Дата" />
        <text>Выбрано: {{ value }}</text>
    </div>
</window>
```

И опишите компонент в `src/lib.rs`:

```rust
use light::prelude::*;

#[template("templates/datepicker.html")]
#[proxy]
pub struct DatePicker {
    #[default(String::new())]
    pub value: String,
}

impl DatePicker {
    /// Текущее выбранное значение.
    pub fn selected(&self) -> String {
        self.value()
    }
}
```

Макрос `#[template]` компилирует HTML в код построения виртуального дерева, а
`#[proxy]` создаёт реактивные геттеры и сеттеры. Пользователь компонента может
запустить его как обычное приложение:

```rust
fn main() -> light::Result<()> {
    light::run(DatePicker::new())
}
```

## 4. Метаданные компонента

Для поиска через `light add` достаточно имени с префиксом `light-fw-`: версия
берётся с crates.io. Секция `[package.metadata.light]` нужна инструментам
Light (`doctor`, `check`, будущему каталогу), если крейт является полноценным
компонентом:

```toml
[package.metadata.light]
kind = "widget"
display-name = "Date Picker"
core = "^0.6"

[package.metadata.light.registry]
official = false
tags = ["date", "input", "calendar"]
```

Если компонент добавляет новые теги, атрибуты или директивы, перечислите их в
`provides`:

```toml
[package.metadata.light.provides]
elements = ["datepicker"]
```

Обязательные поля манифеста и их смысл описаны в
[docs/COMPONENT_AUTHORING.md](COMPONENT_AUTHORING.md).

## 5. Проверка перед публикацией

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
cargo publish --dry-run
```

Убедитесь, что в `Cargo.toml` есть `description`, `license`, `repository` и
`readme` — crates.io требует их для публикации.

## 6. Публикация

```bash
cargo login
cargo publish
```

После публикации компонент можно найти и подключить:

```bash
light search datepicker
light add light-fw-datepicker
```

`light add` добавит зависимость в `Cargo.toml`, включит фичу компонента,
обновит `light.lock` и проверит сборку.

## 7. Обновление версии

Для новой версии поднимите `version` в `Cargo.toml`, обновите `CHANGELOG.md`
и повторите `cargo publish`. Пользователи обновляются командой:

```bash
light update
```
