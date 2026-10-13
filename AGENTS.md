# AGENTS.md - Guidelines for AI agents

Этот файл содержит инструкции для AI-ассистентов (OpenCode, Cursor, Copilot и др.), работающих с этим репозиторием. Цель — обеспечить высокое качество, консистентность и безопасность генерируемого кода.

## Build, Lint, and Test Commands

Используйте эти команды для проверки и валидации изменений.

```bash
# Форматирование кода (Rust)
cargo fmt --all

# Линтинг (Clippy с строгими предупреждениями)
cargo clippy --all-targets --all-features -- -D warnings

# Сборка всех целей (bin, lib, tests)
cargo build --all-targets

# Запуск всех тестов
cargo test --all-features

# Запуск одного теста по имени
cargo test --all-features <test_name>

# Запуск тестов в конкретном модуле
cargo test --all-features module::tests
```

**Полная проверка перед коммитом:**
```bash
cargo fmt --all && cargo clippy --all-targets --all-features -- -D warnings && cargo test --all-features
```

## Code Style Guidelines

### Форматирование
- Используйте `cargo fmt` с настройками из `.rustfmt.toml`.
- Отступы: 4 пробела.
- Длина строки: 100 символов (настраивается).

### Именование
- **Типы**: `PascalCase` (например, `ClientConfig`, `ParseError`).
- **Функции и методы**: `snake_case` (например, `parse_input`, `is_valid`).
- **Константы**: `SCREAMING_SNAKE_CASE` (например, `MAX_RETRIES`).
- **Модули**: `snake_case` (например, `file_reader`).

### Импорты
Группируйте импорты в порядке: `std` → внешние крейты → `crate` (локальные).
```rust
use std::{collections::HashMap, time::Duration};
use serde::{Serialize, Deserialize};
use crate::{config::Config, error::AppError};
```

### Обработка ошибок
- **Библиотечный код**: используйте `thiserror` для определения типов ошибок. **Не используйте `anyhow` в библиотеках**.
- **Прикладной код (binary)**: допускается `anyhow` для упрощения.
- Избегайте `unwrap()` и `expect()` в production-коде. Используйте `?` и типы `Result`.
- Для ошибок с контекстом используйте `#[source]` или `#[from]`.

Пример:
```rust
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Failed to read config: {0}")]
    ConfigRead(#[from] std::io::Error),

    #[error("Invalid value: {value}")]
    InvalidValue { value: String },
}
```

### Типы и сериализация
- Используйте `serde` с `derive` для JSON-сериализации.
- Предпочитайте строго типизированные структуры вместо `serde_json::Value`.
- Для строковых параметров используйте `impl Into<String>` для гибкости.

### Документация
- Публичные элементы должны иметь doc-комментарии (`///`).
- Примеры в doc-комментариях должны компилироваться (они проверяются через `cargo test`).

## Development Workflow

1. **Планирование**: перед написанием кода составьте детальный план.
2. **Реализация**: пишите идиоматичный Rust-код, следуя принципам минимизации аллокаций и потерь данных.
3. **Улучшение**: после первой реализации удалите лишний код, ненужные аллокации, отрефакторите преобразования типов в `From`/`Into`.
4. **Проверка**: запустите полную проверку (fmt + clippy + test).
5. **Коммит**: используйте Conventional Commits (`feat:`, `fix:`, `refactor:`, `test:`, `docs:`).

## CI/CD и проверки

- CI запускает `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test --all-features`.
- Все новые предупреждения Clippy должны быть исправлены, а не подавлены.
- Не добавляйте `#[allow(...)]` без веской причины и комментария.

## Запрещённые практики

- ❌ `unwrap()` / `expect()` в production-коде без крайней необходимости.
- ❌ `panic!()` для обработки ожидаемых ошибок.
- ❌ `unsafe` без явного обоснования и проверки.
- ❌ Хардкод значений, которые могут быть параметрами.
- ❌ Игнорирование предупреждений компилятора и Clippy.
- ❌ Добавление зависимостей без необходимости (проверяйте уже имеющиеся).

## Дополнительные ресурсы

- [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- [Clippy Lints](https://rust-lang.github.io/rust-clippy/master/index.html)
- [Rust Performance Book](https://nnethercote.github.io/perf-book/)