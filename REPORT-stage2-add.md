# Отчёт: Stage 2 — light add with conflict resolution and light.lock

**Коммит:** `cd383cb` — `feat(cli): stage 2 — light add with conflict resolution and light.lock (+ docs)`
**Ветка:** `release/0.19.0` → pushed → https://github.com/gifoff/light/actions/runs/43734277865
**CI:** 15/15 green (2026-11-15T16:19:10Z)

---

## 1. Кодовые изменения (diff)

### 1.1 `crates/light-cli-core/src/lockfile.rs` (+118)

Добавлена секция `[plugins]` в `light.lock`:

```rust
// Поле в struct Lockfile:
/// Приоритет по тегам элемента при конфликте: тег → победитель (имя крейта).
#[serde(default)]
pub plugins: BTreeMap<String, String>,

// Методы:
pub fn set_plugin_priority(&mut self, tag: &str, crate_name: &str) {
    self.plugins.insert(tag.to_string(), crate_name.to_string());
}

pub fn plugin_priority(&self, tag: &str) -> Option<&str> {
    self.plugins.get(tag).map(String::as_str)
}
```

Ключевое изменение в `from_project`: приоритеты **переносятся** из
существующего `light.lock` (не выводятся из `Cargo.toml`):

```rust
let existing_plugins = Self::read(project_root)
    .map(|lf| lf.plugins)
    .unwrap_or_default();
// ...
plugins: existing_plugins,
```

4 новых теста:
- `set_plugin_priority_inserts_and_overwrites` — вставка и перезапись
- `old_lockfile_without_plugins_parses_empty` — обратная совместимость
- `plugins_serialize_to_named_keys` — сериализация в TOML
- `from_project_preserves_plugins_from_existing_lock` — перенос при регенерации

### 1.2 `crates/light-cli-core/src/error.rs` (+27)

Переделан вариант `Conflict` из строкового в структурированный:

```rust
// Было:
Conflict(String),

// Стало:
#[error(
    "конфликт: `{tag}` уже предоставлен компонентом `{existing_crate}`; \
     добавить `{new_crate}`? используйте --force"
)]
Conflict {
    tag: String,           // элемент из provides.elements (или имя конфликтанта)
    existing_crate: String, // уже установленный владелец
    new_crate: String,      // добавляемый
},
```

### 1.3 `crates/light-cli-core/src/add.rs` (+221)

- `AddOptions.force: bool` — флаг принудительного разрешения конфликта.
- `AddPlan.plugin_priorities: Vec<(String, String)>` — теги, чьи приоритеты
  нужно записать в `[plugins]`.
- `pub(crate) fn check_tag_conflict(component, project_root, opts)` —
  возвращает `Vec<(tag, new_crate)>` для `--force` или `Err(Error::Conflict{..})`
  без `--force`. Также проверяет `conflicts` из манифеста.
- В `do_add`: после записи Cargo.toml — блок записи lock-файла с приоритетами:
  ```rust
  let mut lock = Lockfile::from_project(project_root)?;
  for (tag, winner) in &priorities {
      lock.set_plugin_priority(tag, winner);
  }
  lock.write(project_root)?;
  ```
- 3 новых hermetic-теста:
  - `load_installed_from_json_detects_provides_elements` — JSON-path
  - `check_tag_conflict_pure_logic_with_fake_project` — прямая логика
  - `tag_conflict_fixture` — хелпер для fixture-проектов

### 1.4 `crates/light/src/cmd_add.rs` (+114)

- `AddArgs.force: bool` — clap flag `--force`.
- `AddArgs` теперь `#[derive(Clone)]` для ретрая.
- `run(args)` → `try_add(&args)`, при `Error::Conflict` →
  `resolve_conflict_interactively(...)`.
- `try_add(args: &AddArgs) -> anyhow::Result<i32>` — основной поток (отдельно
  для повторяемости при ретрае с `force=true`).
- `resolve_conflict_interactively`:
  - версия mismatch (existing == new) → plain error
  - `--force`/`--yes` → immediate retry force=true
  - non-TTY → anyhow error + подсказка
  - TTY → интерактивный prompt [1/2], `read_line`, loop до валидного ввода

---

## 2. Документация (после «Применяй»)

### `docs/cli.md` (L74–82 → L74–99)

- Заголовок: `light add КОМПОНЕНТ [версия] [--force]`
- Добавлен абзац про разрешение конфликтов
- Пример: `light add datepicker --force`
- Таблица флагов: `--version <VER>`, `--force`

### `docs/guide.md` (L448–470 → L448–485)

- Таблица флагов: добавлена строка `--force`
- Новый абзац после примера с `light-fw-datepicker`:
  три сценария (интерактив / неинтерактив / `--force`)

---

## 3. E2E логи

Каталог: `C:\Temp\light-plugin-e2e\stage2-add\`

### Шаг 1: `light init my-app` → OK (shots/01-init.txt)

### Шаг 2: `light add light-fw-widget-datepicker --yes --no-verify`

```
План добавления light-fw-widget-datepicker
  источник:    vendor (C:\Temp\...\my-app\.\vendor\light-fw-widget-datepicker)
  версия:      0.2.0
  фича:        widget-datepicker (будет создана)
  зависимость: light-fw-widget-datepicker = { path = "vendor\light-fw-widget-datepicker", optional = true }
✓ light-fw-widget-datepicker добавлен (фича `widget-datepicker`) (без проверки сборки)
```
(shots/02-add-widget.txt)

**Path-dep в Cargo.toml подтверждён** (Задача 4):
```toml
light-fw-widget-datepicker = { path = 'vendor\light-fw-widget-datepicker', optional = true }
```

### Шаг 3: `light add light-fw-datepicker --yes --no-verify` (конфликт!)

```
⚠️ Конфликт: тег `datepicker` уже предоставлен компонентом `light-fw-widget-datepicker`.
   Новый компонент `light-fw-datepicker` тоже предоставляет `datepicker`.
   Продолжаю с --force (приоритет будет записан в light.lock).
План добавления light-fw-datepicker
  ...
✓ light-fw-datepicker добавлен (фича `widget-datepicker`) (без проверки сборки)
  приоритет: datepicker → light-fw-datepicker (зафиксировано в light.lock)
```
(shots/03-add-datepicker-conflict.txt)

**`--yes` автоматически разрешает конфликт через `force=true`** — как
проектировано: CI-режим с `--yes` не падает на конфликте.

### Шаг 4: Non-TTY conflict (separate project `my-app2`, no `--yes`)

```
⚠️ Конфликт: тег `datepicker` уже предоставлен компонентом `light-fw-widget-datepicker`.
   Новый компонент `light-fw-datepicker` тоже предоставляет `datepicker`.
   В неинтерактивном режиме добавьте `--force`, если хотите использовать
   `light-fw-datepicker` вместо `light-fw-widget-datepicker`.
конфликт по тегу `datepicker`: выберите --force (использовать новый) или запустите интерактивно
```
(shots/06-nontty-conflict-error.txt)

### Итоговые файлы:

**Cargo.toml** (shots/04-cargo-toml.txt):
```toml
[dependencies]
light-fw = "0.18"
light-fw-widget-datepicker = { path = 'vendor\light-fw-widget-datepicker', optional = true }
light-fw-datepicker = { path = 'vendor\light-fw-datepicker', optional = true }

[features]
widget-datepicker = ["dep:light-fw-widget-datepicker", "dep:light-fw-datepicker"]
```

**light.lock** (shots/05-light-lock.txt):
```toml
[plugins]
datepicker = "light-fw-datepicker"
```

> **Примечание о DSH-песочнице:** `cargo metadata` в песочнице возвращает
> пустой stdout → `load_installed` получает `[]`. Конфликт в шаге 3 тем не
> менее сработал, потому что `check_tag_conflict` читает `provides.elements`
> напрямую из Cargo.toml компонентов (не через cargo metadata). Путь
> `load_installed` через JSON-метаданные покрыт hermetic-тестом
> `load_installed_from_json_detects_provides_elements`.

---

## 4. Локальные гейты

| Гейт | Результат |
|------|-----------|
| `cargo fmt --all -- --check` | ✅ clean (0 строк вывода) |
| `cargo clippy --workspace --all-targets --no-default-features --release -- -D warnings` | ✅ clean (exit 0, 41s) |
| `cargo test --workspace --all-features --release -j 2 --no-fail-fast` | ✅ 262 lib + 12 integration + 49 CLI + все другие — pass. 2 pre-existing avatar failures in example-dashboard (PermissionDenied creating temp files — НЕ наш код) |

Лог: `C:\Temp\stage2-test.log`

---

## 5. CI

**Run:** https://github.com/gifoff/light/actions/runs/43734277865

| # | Job | Результат |
|---|-----|-----------|
| 1 | Clippy (ubuntu-latest) | ✅ success |
| 2 | Tests (windows-latest) | ✅ success |
| 3 | Documentation | ✅ success |
| 4 | Clippy (macos-latest) | ✅ success |
| 5 | Tests (ubuntu-latest) | ✅ success |
| 6 | Build examples (default features) | ✅ success |
| 7 | Features (a11y-no-winit) | ✅ success |
| 8 | Features (full) | ✅ success |
| 9 | Features (basic-window-a11y) | ✅ success |
| 10 | Features (form-window-a11y) | ✅ success |
| 11 | Features (basic-elements) | ✅ success |
| 12 | Formatting | ✅ success |
| 13 | Features (no-default-features) | ✅ success |
| 14 | Tests (macos-latest) | ✅ success |
| 15 | Clippy (windows-latest) | ✅ success |

**Итого: 15/15 green.**

---

## 6. Замечания и gaps

1. **DSH sandbox**: `cargo metadata` spawn → пустой stdout → fallback
   `load_installed = []`. В E2E конфликт всё равно сработал (через прямое
   чтение `provides` из manifests), но в production-среде (вне DSH) путь через
   `cargo metadata` тоже работает — покрыт hermetic-тестом.

2. **Положительная ветка `--force`** (приоритеты пишутся в lock-файл)
   не изолирована в отдельном hermetic-тесте на уровне `do_add` — покрыта E2E
   (шаг 3 выше: `приоритет: datepicker → light-fw-datepicker (зафиксировано
   в light.lock)`) + unit-тестами `set_plugin_priority` и
   `from_project_preserves_plugins_from_existing_lock`.

3. **Pre-existing failures**: `example-dashboard` → `avatar_oversized_dimensions_are_rejected`
   и `avatar_too_large_is_rejected` — PermissionDenied при создании temp-файлов
   в DSH-песочнице. Не связаны с настоящим изменением.
