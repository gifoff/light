# AGENTS.md - Guidelines for AI agents

Этот файл содержит инструкции для AI-ассистентов (OpenCode, Cursor, Copilot и др.), работающих с этим репозиторием. Цель — обеспечить высокое качество, консистентность и безопасность генерируемого кода.

## 🚨 Главные правила (TL;DR)

1. **Никогда не коммитьте сломанный код.** Если правка затрагивает несколько слоёв (vdom, layout, paint, runtime) — это **один** компилируемый коммит.
2. **Всегда проверяйте `git status --short` перед `light publish`**. Публикация с "грязным" деревом приведёт к сбою и рассинхронизации версий на crates.io.
3. **Правило доказательств (UI/CLI)**: если вы меняете `layout`, `paint`, `init` или `dev`, вы **обязаны** приложить скриншот или лог успешного запуска (smoke-тест). Код без визуального подтверждения не принимается.
4. **Обновление версий**: Bump версии делается отдельным коммитом `chore(release): bump to X.Y.Z`, после чего ставится тег и только потом запускается публикация.
5. **Документация — это код**: Перед началом работы **прочитайте** `docs/` и `README.md`, чтобы понять контекст. Если вы меняете поведение, API, добавляете или удаляете фичи — вы **обязаны** обновить соответствующие разделы документации в том же коммите.

## Documentation-First Policy (Правило документации)

В этом проекте документация не является второстепенной. Любое изменение в коде должно быть отражено в ней.

1. **Чтение перед правками**: Перед тем как предлагать архитектурные изменения, добавление новых элементов, фич или модификацию CLI, агент **обязан** изучить существующие документы (`docs/ARCHITECTURE.md`, `docs/COMPONENT_MODEL.md`, `docs/guide.md`), чтобы не нарушить заложенные принципы.
2. **Синхронизация**: Если вы:
   - добавили новую публичную функцию (например, `run_with_config`) — обновите `README.md`, `docs/quickstart.md` и `docs/guide.md`;
   - изменили CLI-команду или флаги — обновите `docs/guide.md` и `docs/RELEASING.md`;
   - добавили новую Cargo-фичу — обновите `docs/guide.md` (раздел «Установка и Cargo-фичи») и `docs/DESIGN_PRINCIPLES.md`;
   - изменили структуру крейтов — обновите `docs/ARCHITECTURE.md`.
3. **Никаких «фантомных» фич**: Запрещено описывать в документации то, чего нет в коде, или оставлять в доках упоминания удалённых функций.

## Build, Lint, and Test Commands

Используйте эти команды для проверки и валидации изменений.

```bash
# Форматирование кода (Rust)
cargo fmt --all

# Линтинг (Clippy со строгими предупреждениями)
cargo clippy --workspace --all-targets --all-features -- -D warnings

# Сборка всех целей (bin, lib, tests)
cargo build --all-targets

# Запуск всех тестов
cargo test --all-features
```

### Особенности Windows (MSVC)
На Windows из-за лимита длины командной строки (D8050) сборка крейта `ring` (через `reqwest` в `light-fw-cli`) может падать при параллельной сборке.
Обход:
```powershell
$env:CARGO_BUILD_JOBS = "1"
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features -j 2
```

## Development Workflow (Правильная последовательность)

1. **Изучение контекста**: Прочитайте `docs/ARCHITECTURE.md` и релевантные разделы `docs/guide.md` перед планированием.
2. **Планирование**: Составьте детальный план изменений.
3. **Реализация**: Пишите идиоматичный Rust-код, следуя принципам минимизации аллокаций.
4. **Локальные гейты**:
   - `cargo fmt --all`
   - `cargo build --workspace --exclude light-fw-cli --exclude light-fw-cli-core` (если `ring` блокирует сборку).
   - `cargo clippy`
   - `cargo test`
5. **Smoke Test (для UI/CLI)**:
   - Если менялся `init`, `dev`, `layout` или `runtime`:
   - Создайте чистый проект: `light init test-smoke`
   - Запустите `light dev` в нём.
   - Убедитесь, что окно открывается, текст рендерится, DevTools запускается.
   - Сделайте скриншот.
6. **Обновление документации (Obligatory)**:
   - Внесите правки в `README.md` и `docs/` (см. *Documentation-First Policy*).
   - Убедитесь, что оглавление (TOC) в измененных markdown-файлах осталось консистентным.
7. **Коммит**: Используйте Conventional Commits (`feat:`, `fix:`, `refactor:`, `test:`, `docs:`). В сообщении коммита укажите, если были обновлены доки.

## Release Workflow (Публикация)

**Категорически запрещено** публиковать крейты, если `git status --short` не пуст.

1. **Bump версий**: Обновите версии во всех `Cargo.toml` и `Cargo.lock`.
2. **CHANGELOG**: Добавьте раздел в `CHANGELOG.md` (формат Keep a Changelog).
3. **Коммит и тег**:
   ```bash
   git add -A
   git commit -m "chore(release): bump to X.Y.Z"
   git tag -a vX.Y.Z -m "Release X.Y.Z: <features>"
   git push origin master
   git push origin vX.Y.Z
   ```
4. **Публикация**:
   ```bash
   light publish --yes --skip-gates --no-verify
   ```
5. **Верификация**: Проверьте, что все крейты опубликованы:
   ```bash
   cargo search light --limit 1
   ```

## Code Style Guidelines

### Форматирование
- Используйте `cargo fmt` с настройками из `.rustfmt.toml`.
- Отступы: 4 пробела. Длина строки: 100 символов.

### Именование
- **Типы**: `PascalCase` (`ClientConfig`, `ParseError`).
- **Функции и методы**: `snake_case` (`parse_input`, `is_valid`).
- **Константы**: `SCREAMING_SNAKE_CASE` (`MAX_RETRIES`).
- **Модули**: `snake_case` (`file_reader`).

### Импорты
Группируйте импорты в порядке: `std` → внешние крейты → `crate` (локальные).
```rust
use std::{collections::HashMap, time::Duration};
use serde::{Serialize, Deserialize};
use crate::{config::Config, error::AppError};
```

### Обработка ошибок
- **Библиотечный код**: используйте `thiserror`. **Не используйте `anyhow` в библиотеках**.
- **Прикладной код (binary)**: допускается `anyhow`.
- Избегайте `unwrap()` и `expect()` в production-коде. Используйте `?`.

### Типы и сериализация
- Используйте `serde` с `derive` для JSON-сериализации.
- Предпочитайте строго типизированные структуры вместо `serde_json::Value`.
- Для строковых параметров используйте `impl Into<String>`.

### Документация
- Публичные элементы должны иметь doc-комментарии (`///`).
- Примеры в doc-комментариях должны компилироваться (проверяются через `cargo test`).

## Запрещённые практики

- ❌ `unwrap()` / `expect()` в production-коде без крайней необходимости.
- ❌ `panic!()` для обработки ожидаемых ошибок.
- ❌ `unsafe` без явного обоснования.
- ❌ Хардкод значений, которые могут быть параметрами.
- ❌ Игнорирование предупреждений компилятора и Clippy.
- ❌ **Публикация (`light publish`) без чистого `git status`.**
- ❌ **Правки в `layout.rs` или `runtime.rs` без визуального подтверждения (скриншота).**
- ❌ **Коммит изменений в коде без обновления соответствующей документации (если изменился API, добавлена фича или поменялось поведение).**

## Дополнительные ресурсы

- [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- [Clippy Lints](https://rust-lang.github.io/rust-clippy/master/index.html)
- [Rust Performance Book](https://nnethercote.github.io/perf-book/)