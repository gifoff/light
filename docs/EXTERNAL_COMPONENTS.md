# Сторонние компоненты Light

Короткий практический гайд: как создать, опубликовать и подключить собственный компонент Light на примере `light-fw-datepicker`.

Более подробно — в [COMPONENT_AUTHORING.md](COMPONENT_AUTHORING.md) и [COMPONENT_MODEL.md](COMPONENT_MODEL.md).

## 1. Создание крейта

Имя компонента должно начинаться с `light-fw-`: по этому префиксу `light add` и `light search` отличают компоненты Light от обычных крейтов.

```bash
cargo new light-fw-datepicker --lib
cd light-fw-datepicker
```

## 2. Зависимость от Light

Добавьте мета-крейт `light-fw` в `Cargo.toml`:

```toml
[package]
name = "light-fw-datepicker"
version = "0.1.0"
edition = "2021"
rust-version = "1.95"
license = "MIT OR Apache-2.0"
description = "Date picker component for Light"
repository = "https://github.com/<user>/light-fw-datepicker"

[dependencies]
light-fw = "0.17"

# Для поддержки локальной разработки с Hot Reload
[features]
dev-mode = ["light-fw/dev-mode"]
```

Если компонент использует элементы формы, подключите `light-fw-element-form` и фичу `light-fw/elements-form`:

```toml
[dependencies]
light-fw = { version = "0.17", features = ["elements-form"] }
light-fw-element-form = "0.17"
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
use light_fw::prelude::*;

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

Если вы запускаете компонент как самостоятельное приложение, используйте `run_with_config` для инициализации DevTools:

```rust
fn main() -> light_fw::Result<()> {
    light_fw::run_with_config("light.config.json", DatePicker::new())
}
```

## 4. Метаданные компонента

Для поиска через `light add` достаточно имени с префиксом `light-fw-`. Секция `[package.metadata.light]` нужна инструментам Light (`doctor`, `check`, будущему каталогу), если крейт является полноценным компонентом:

```toml
[package.metadata.light]
kind = "widget"
display-name = "Date Picker"
core = "^0.17"

[package.metadata.light.registry]
official = false
tags = ["date", "input", "calendar"]
```

Если компонент добавляет новые теги, атрибуты или директивы, перечислите их в `provides`:

```toml
[package.metadata.light.provides]
elements = ["datepicker"]
```

## 5. Проверка перед публикацией

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
cargo publish --dry-run
```

Убедитесь, что в `Cargo.toml` есть `description`, `license`, `repository` и `readme`.

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

`light add` добавит зависимость в `Cargo.toml`, включит фичу компонента, обновит `light.lock` и проверит сборку.

## 7. Обновление версии

Для новой версии поднимите `version` в `Cargo.toml`, обновите `CHANGELOG.md` и повторите `cargo publish`. Пользователи обновляются командой:

```bash
light update
```