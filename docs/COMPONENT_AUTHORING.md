# Написание компонентов Light

Практическое руководство по созданию собственных компонентов Light: от пустого крейта до публикации в crates.io. Архитектуру компонентной модели описывает [COMPONENT_MODEL.md](COMPONENT_MODEL.md); здесь — конкретные шаги, манифест и примеры кода.

- [1. Введение](#1-введение)
- [2. Типы компонентов и их назначение](#2-типы-компонентов-и-их-назначение)
- [3. Шаг 1: Создание проекта](#3-шаг-1-создание-проекта)
- [4. Шаг 2: Манифест `Cargo.toml`](#4-шаг-2-манифест-cargotoml)
- [5. Шаг 3: Реализация компонента](#5-шаг-3-реализация-компонента)
- [6. Шаг 4: Тестирование](#6-шаг-4-тестирование)
- [7. Шаг 5: Подготовка к публикации](#7-шаг-5-подготовка-к-публикации)
- [8. Лучшие практики](#8-лучшие-практики)

---

## 1. Введение

**Компонент Light** — это автономный Rust-крейт, который добавляет фреймворку функциональность: новые элементы разметки, поведение существующих элементов, готовые составные виджеты или интеграции с ОС.

## 2. Типы компонентов и их назначение

| `kind`        | Назначение                                              | Что уже есть                        |
| ------------- | ------------------------------------------------------- | ----------------------------------- |
| `element`     | Новые базовые элементы разметки (теги)                  | `light-fw-element-form`             |
| `behavior`    | Расширение существующих элементов (атрибуты, директивы) | `light-fw-behavior-validation`      |
| `widget`      | Составные компоненты из elements + behaviors            | —                                   |
| `integration` | Мост с ОС и внешними сервисами (Rust-API, не теги)      | —                                   |
| `subsystem`   | Крупные подсистемы фреймворка                           | —                                   |
| `dev`         | Только для разработки, вне релизной сборки              | —                                   |

## 3. Шаг 1: Создание проекта

Создайте библиотечный крейт:

```powershell
cargo new light-widget-my-widget --lib
cd light-widget-my-widget
```

## 4. Шаг 2: Манифест `Cargo.toml`

Пример для виджета:

```toml
[package]
name = "light-widget-datepicker"
version = "0.1.0"
edition = "2021"
rust-version = "1.95"
license = "MIT OR Apache-2.0"
description = "Виджет выбора даты для Light"
repository = "https://github.com/<user>/light-widget-datepicker"

[dependencies]
# Крейт `light` реэкспортирует ядро и макросы `#[template]`/`#[proxy]`.
light = { version = "0.15", default-features = false, features = [
    "elements-basic",
    "elements-form",
] }
light-fw-element-form = { version = "0.15" }

[package.metadata.light]
kind = "widget"
display-name = "Date Picker"
# Актуальная мажорная версия ядра
core = "^0.15"

[package.metadata.light.provides]
elements = ["datepicker"]
attributes = []
directives = []

[package.metadata.light.requires]
components = [
    "light-fw-element-form ^0.15",
]
```

### Требования к зависимостям и макросам
Макросы `#[template]` и `#[proxy]` разворачиваются в пути `::light::…`, поэтому **зависимость обязана называться `light`**.

## 5. Шаг 3: Реализация компонента

### Виджет на шаблоне и свойствах (идиоматично)

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

Макрос `#[proxy]` для каждого поля генерирует геттер (`value()`) и сеттер (`set_value(...)`).

Если ваш компонент является самостоятельным приложением, используйте в `main.rs`:

```rust
fn main() -> light::Result<()> {
    // Обязательно для поддержки DevTools и light.config.json
    light::run_with_config("light.config.json", DatePicker::new())
}
```

## 6. Шаг 4: Тестирование

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

Проверка через CLI в проекте-потребителе:

```powershell
light check --strict
```

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

### Публикация

```powershell
cargo publish
```

## 8. Лучшие практики

- **Именование.** Префикс отражает тип: `light-element-<name>`, `light-behavior-<name>`, `light-widget-<name>`.
- **Версионирование.** Следуйте semver. Поле `core` — диапазон совместимости с ядром.
- **Зависимости.** Минимизируйте их. Используйте `default-features = false`.
- **Реактивность.** `#[proxy]` даёт геттеры/сеттеры, а `#[template]` компилируется на этапе сборки — ошибки разметки видны компилятору.
