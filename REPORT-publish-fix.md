# REPORT-publish-fix: Фикс циклической dev-зависимости и завершение публикации 0.19.0

Дата: 2026-11-16
Статус: **Публикация завершена, CI зелёный (15/15)**

---

## 1. Diff: `crates/light-macros/Cargo.toml` + новые файлы в `crates/light-core/tests/`

Коммит: [`4e5a734`](https://github.com/gifoff/light/commit/4e5a734) — `fix(macros): remove circular dev-dependency light-fw; move trybuild to light-core`

### Изменённые файлы

```diff
# crates/light-macros/Cargo.toml
 [dev-dependencies]
 trybuild = "1"
-light-fw = { workspace = true }

# crates/light-core/Cargo.toml
 [dev-dependencies]
 criterion = "0.5"
 insta = { version = "1", features = ["json"] }
 serde_json = "1"
+trybuild = "1"
```

### Удалённые файлы

- `crates/light-macros/tests/ui.rs`
- `crates/light-macros/tests/ui/component_unknown_tag.rs`
- `crates/light-macros/tests/ui/component_unknown_tag.stderr`

### Перемещённые файлы

- `crates/light-macros/templates/ui_unknown_tag.html` → `crates/light-core/templates/ui_unknown_tag.html`
- `crates/light-macros/tests/ui/component_unknown_tag.{rs,stderr}` → `crates/light-core/tests/ui/component_unknown_tag.{rs,stderr}`

### Новый файл: `crates/light-core/tests/ui.rs`

```rust
//! Trybuild-тесты: проверяют `compile_error!` на неизвестные кастомные теги
//! в HTML-шаблонах (ТЗ 0.19.0 Этап 4, Plugin API).

#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/component_unknown_tag.rs");
}
```

### Обновлённый fixture (после всех фиксов)

`crates/light-core/tests/ui/component_unknown_tag.rs`:
```rust
use light_fw_core::template;

#[template("../../../../crates/light-core/templates/ui_unknown_tag.html")]
struct UiApp {
    title: String,
}

fn main() {}
```

`crates/light-core/tests/ui/component_unknown_tag.stderr` (финальная версия, LF, без BOM):
```
error: неизвестный элемент <datepicker>; поддерживаются: <div>, <span>, <h1>…<h5>, ...
  --> tests/ui/component_unknown_tag.rs:14:1
   |
14 | #[template("../../../../crates/light-core/templates/ui_unknown_tag.html")]
   | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
   |
   = note: this error originates in the attribute macro `template` (in Nightly builds, run with -Z macro-backtrace for more info)
```

### Доп. коммиты

| Коммит | Описание |
|--------|----------|
| [`4147e22`](https://github.com/gifoff/light/commit/4147e22) | fix(test): correct trybuild template path depth (4 levels, not 5) |
| [`9a779aa`](https://github.com/gifoff/light/commit/9a779aa) | fix(test): remove UTF-8 BOM from trybuild .stderr file |
| [`58c9605`](https://github.com/gifoff/light/commit/58c9605) | fix(ci): force LF line endings via .gitattributes |
| [`3f7305a`](https://github.com/gifoff/light/commit/3f7305a) | fix(test): correct trybuild .stderr indentation for 2-digit line numbers |

---

## 2. Локальный вывод `cargo test -p light-fw-core --test ui`

Локальный прогон **невозможен** на данной Windows-машине из-за окружения:

- `ring` (MSVC) падает с D8050 при параллельной сборке
- `C:\Temp` ACL: Os error 5 / LNK1104 при линковке temp-файлов trybuild
- trybuild создаёт изолированный проект в `target/tests/trybuild/`, inner-компиляция падает на этапе линковки `proc-macro2`

Валидация .stderr выполнена через **CI (Linux + macOS + Windows)**:

- Перед фиксами: [run 43932455103](https://github.com/gifoff/light/actions/runs/43932455103) — FAILED (path 5 vs 4)
- После фикса пути: [run 43934330908](https://github.com/gifoff/light/actions/runs/43934330908) — FAILED (BOM)
- После фикса BOM: [run 43935795506](https://github.com/gifoff/light/actions/runs/43935795506) — FAILED (indentation 4 vs 3 spaces)
- **Финальный:** [run 43939739245](https://github.com/gifoff/light/actions/runs/43939739245) — **SUCCESS 15/15** ✓

---

## 3. `cargo search` после публикации

### `cargo search light`

```
light = "0.19.0"    # Утилита light: проверка проекта и управление компонентами Light
... and 8098 crates more
```

### `cargo search light-fw`

```
light-fw = "0.19.0"                        # Реактивный графический интерфейс для Rust
light-fw-html = "0.19.0"                   # Разбор HTML-шаблонов Light для процедурных макросов
light-fw-macros = "0.19.0"                 # Процедурные макросы библиотеки Light: #[proxy] и #[template]
light-fw-core = "0.19.0"                   # Ядро Light: виртуальный DOM, реактивность, раскладка, отрисовка и окно
light-fw-cli-core = "0.19.0"              # Ядро CLI Light: анализ проекта, управление компонентами и light.lock
light-fw-behavior-validation = "0.19.0"   # Компонент Light: валидация форм
light-fw-element-form = "0.19.0"          # Компонент Light: элементы формы
light = "0.19.0"                          # Утилита light
light-fw-devtools-web = "0.19.0"          # Web UI DevTools: браузерный инспектор VDOM/State
```

**Все 9 крейтов на версии 0.19.0** ✓

---

## 4. CI Run URL

- **Зелёный (финальный):** https://github.com/gifoff/light/actions/runs/43939739245 — **15/15 success**
- Круг предшествующих (для истории):
  - https://github.com/gifoff/light/actions/runs/43932455103 — failure (path depth)
  - https://github.com/gifoff/light/actions/runs/43934330908 — failure (BOM)
  - https://github.com/gifoff/light/actions/runs/43935795506 — failure (indentation)
  - https://github.com/gifoff/light/actions/runs/43938531679 — failure (still whitespace)

---

## 5. Путь к отчёту

Этот файл: `REPORT-publish-fix.md` (корень репозитория)

---

## GitHub Release

Создание GitHub Release **не выполнено**: токен `GH_TOKEN` не имеет scope `repo:write`.
Обе попытки (`gh release create` и `POST /repos/gifoff/light/releases`) вернули HTTP 403.

Готовая команда для ручного выполнения (нужен токен с `repo:write`):

```bash
gh release create v0.19.0 \
  --title "Release 0.19.0: Plugin API v1" \
  --notes "## Plugin API v1

### Added
- \`#[component]\` — макрос для объявления пользовательских элементов (Plugin API)
- \`light-fw-element-form\` — новый крейт с формой и валидацией
- \`light check\` — проверка проекта на использование неизвестных тегов
- \`light dev --agent\` — headless-инструменты для автономного тестирования UI
- \`light devtools shot/find/click/type/key/state/tree\` — DevTools CLI
- Web UI на \`http://127.0.0.1:9228\` при запуске \`light dev\`
- Центрирование \`<h1>\`–\`<h5>\` в \`align-items=center\`
- trybuild UI-тест для ошибок неизвестных тегов

### Fixed
- Циклическая dev-зависимость light-fw-macros → light-fw
- Путь trybuild fixture (4 уровня, не 5)
- UTF-8 BOM в .stderr файлах
- Индентация .stderr для двузначных номеров строк

### Публикуемые крейты
| Крейт | Версия |
|-------|--------|
| light-fw-html | 0.19.0 |
| light-fw-cli-core | 0.19.0 |
| light-fw-devtools-web | 0.19.0 |
| light-fw-macros | 0.19.0 |
| light-fw-core | 0.19.0 |
| light-fw-behavior-validation | 0.19.0 |
| light-fw-element-form | 0.19.0 |
| light-fw | 0.19.0 |
| light | 0.19.0 |" \
  --latest
```

Теги уже созданы и опубликованы: `v0.19.0` (указывает на merge-коммит `267955d`).
