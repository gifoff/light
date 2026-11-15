# Отчёт: Фикс CI для Stage 1 — устойчивость `light check` к ошибкам cargo metadata

**Дата:** 2026-11-15  
**Ветка:** `release/0.19.0`  
**Коммит:** `146e9a9` — `fix(cli): graceful cargo metadata fallback in light check, robust CLI tests`  
**PR:** [#2](https://github.com/gifoff/light/pull/2)

---

## 1. Проблема

`light check` (Этап 1 Plugin API) вызывал `load_installed()` → `cargo metadata --no-deps`.
Если `cargo metadata` завершался ошибкой (пустой stdout, нет cargo в PATH, битый JSON),
команда падала с:

```
JSON error: EOF while parsing a value at line 1 column 0
```

Это ломало 6 CLI-тестов (`check_*`) как локально (DSH sandbox пустой stdout),
так и на CI-раннерах (GitHub Actions). Результат: **12/15 job RED** в двух
последовательных прогонах (43718247840, 43719670142).

## 2. Корневая причина

`load_installed()` в `crates/light-cli-core/src/metadata.rs` использовал `?`-оператор
для распространения ошибок `cargo metadata` наверх. В CI и в DSH-сэндбоксе
`cargo metadata` возвращает пустой stdout (не сеть, а sandbox-ограничения),
что приводит к `EOF while parsing a value`.

## 3. Фикс (Задача 1 + 2 по ТЗ)

### 3.1. `metadata.rs` — `load_installed` не фатален

Вместо распространения ошибки через `?`:

```rust
pub fn load_installed(project_root: &Path) -> Result<Vec<ComponentMetadata>> {
    let result = (|| -> Result<Vec<ComponentMetadata>> {
        // ... cargo metadata --no-deps + parse ...
    })();
    match result {
        Ok(v) => Ok(v),
        Err(e) => {
            eprintln!("warning: cargo metadata failed: {e}");
            Ok(Vec::new())
        }
    }
}
```

**Семантика:** если `cargo metadata` не сработал — предупреждение на stderr,
возврат пустого вектора. `light check` формирует отчёт по bundled registry
и сканеру шаблонов; раздел «Установлено» будет просто пуст.

**Public API не изменён:** сигнатура `pub fn load_installed(project_root: &Path)
-> Result<Vec<ComponentMetadata>>` сохранена (как требует ТЗ).

### 3.2. `cmd_check.rs` — без изменений

Ошибка больше не доходит до `cmd_check.rs`, т.к. `build_check_report()` получает
`Ok(vec![])` вместо `Err(...)`. Существующий код уже корректно обрабатывает
пустой список установленных компонентов. JSON-режим всегда печатает валидный JSON
с пятью массивами; text-режим печатает «🟢 Установлено: нет».

### 3.3. CLI-тесты — без изменений

Шесть `check_*` тестов в `crates/light/tests/cli.rs` теперь проходят автоматически:
- При успешном `cargo metadata` — данные из реального проекта.
- При ошибке — `installed: []`, что даёт те же строки вывода («Установлено: нет»,
  `unknown`/`available` из bundled registry), которые и проверяют ассерты.

Локальная верификация: **49/49 passed, 0 failed**.

## 4. Локальная верификация

### 4.1. Сборка

```
cargo build --release -p light
→ Finished `release` profile [optimized] target(s) in 47.72s
```

### 4.2. Ручной smoke test

```
$ ./target/release/light check crates/light/tests/fixtures/empty-project
warning: cargo metadata failed: JSON error: EOF while parsing a value at line 1 column 0
🟢 Установлено: нет

$ ./target/release/light check --format json crates/light/tests/fixtures/empty-project
warning: cargo metadata failed: JSON error: EOF while parsing a value at line 1 column 0
{
  "available": [],
  "conflicts": [],
  "installed": [],
  "unknown": [],
  "unused": []
}
```

Ожидаемо: warning на stderr, валидный вывод на stdout, exit code 0.

### 4.3. Тесты CLI

```
$ cargo test -p light --test cli --release
test result: ok. 49 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

Все 6 `check_*` тестов зелёные.

### 4.4. Полный workspace

```
$ cargo test --workspace --all-features --release -j 2 --no-fail-fast
→ Единственный failing target: example-dashboard (2 pre-existing avatar tests,
  EDR environment issue, unrelated to this change)
→ Все остальные ~70 test targets green, включая:
  - light-fw-cli-core: 256 passed
  - check_integration: 12 passed
  - light --test cli: 49 passed
```

### 4.5. Форматирование и линтер

```
cargo fmt --all && cargo fmt --all -- --check   → FMT_OK
cargo clippy --release -p light-fw-cli-core --all-targets -- -D warnings → clean
cargo clippy --release -p light --all-targets -- -D warnings → clean
```

## 5. CI

| Run | Commit | Status | Примечание |
|-----|--------|--------|------------|
| 43718247840 | `a21e413` | ❌ RED 12/15 | Устаревшие `check_*` assertions (до фикса 4a47d5f) |
| 43719670142 | `4a47d5f` | ❌ RED 12/15 | `JSON error: EOF...` — cargo metadata падает на раннерах |
| **43725114456** | **`146e9a9`** | ⏳ in_progress | Ожидается 15/15 |

**URL run'а:** https://github.com/gifoff/light/actions/runs/43725114456

## 6. Изменённые файлы

| Файл | Изменение |
|------|-----------|
| `crates/light-cli-core/src/metadata.rs` | +28/−13: graceful fallback в `load_installed` |

Файлы `check.rs`, `cmd_check.rs`, `cli.rs` **не изменены** — семантика
оставшихся функций совместима с новым поведением.

## 7. Ограничения и отклонения

- **Clippy локально через `--release`:** Windows MSVC D8050 (>8KB cmdline при
  сборке `ring` в debug). Обход: `cargo clippy --release`. Полная проверка
  `--all-features` — только в CI (Linux/macOS/Windows runners).
- **Pre-existing failures:** 2 avatar-теста в `example-dashboard` падают из-за
  EDR-окружения (файловые ограничения на чтение PNG). Не связаны с этим фиксом.

## 8. Итог

Фикс минимальный (1 файл, +28/−13 строк), не меняет public API, не удаляет
тесты, не трогает docs. Целевое поведение: `light check` работает даже когда
`cargo metadata` недоступен, печатает предупреждение и формирует отчёт по
bundled registry.
