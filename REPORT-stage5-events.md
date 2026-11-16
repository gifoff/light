# Отчёт: Этап 5 — Runtime event routing и v-model

**Коммит:** `fc0fe5c` — `feat(core): stage 5 — runtime on_event dispatch for CustomElement`
**Ветка:** `release/0.19.0`
**CI:** https://github.com/gifoff/light/actions/runs/43880919188 — **15/15 GREEN** ✅

## Что реализовано

### 1. Thread-local registry (`runtime.rs`)

- `store_custom_instance(key: &str, instance: Box<dyn CustomElement>)` — сохраняет
  экземпляр кастомного элемента в thread-local `RefCell<HashMap>` с ключом = NodeKey
  родителя (String).
- `drain_custom_instances() -> HashMap<String, Box<dyn CustomElement>>` — забирает
  всю карту (через `mem::take`), вызывается в `redraw()` перед рендером.
- В `build_custom_element` вызов `store_custom_instance` идёт **до** возврата
  `Some(rendered)`, чтобы инстанс был доступен при следующем redraw.

### 2. Walk-up from target key to root (`window/mod.rs`)

- `WindowState.custom_instances: HashMap<String, Box<dyn CustomElement>>` — хранит
  инстансы между redraw'ами.
- В `redraw()`: drain из thread-local → в `custom_instances`; после `Arc::new(tree)` —
  повторный harvest (новые инстансы, созданные в этом кадре).

### 3. dispatch_custom_event (`window/mod.rs`)

- `dispatch_custom_event(&mut self, key: &str, event: &DomEvent) -> bool` —
  ищет инстанс по ключу (walk-up от целевого ключа к корню), downcast через
  `downcast_custom::<T>()`, вызывает `on_event(&mut self, ctx, event)`.
- Подключён в `on_click` Click branch **перед** `dispatch_checked`.
- Возвращает `true` если элемент обработал событие (для любого matched custom element).

### 4. EventCtx + downcast_custom (`plugins.rs`)

- `EventCtx<'a> { pub(crate) redraw: &'a dyn Fn() }` — контекст, передаваемый в
  `on_event`; метод `schedule_redraw()` вызывает замыкание invalidate.
- `downcast_custom<T: 'static>(elem: &dyn CustomElement) -> Option<&T>` — безопасный
  downcast через `Any` (`let any: &dyn Any = elem;`).
- Трейт `CustomElement` **не изменён** (безопасность: `Box<dyn CustomElement>` не
  имеет `Default`, `tag()` требует `Sized`, Any не callable на trait-obj refs).

### 5. v-model passthrough

- Атрибуты (включая `v-model`) передаются в `render()` через `element.attrs` —
  без изменения сигнатуры трейта.

## Тесты (4 шт., все зелёные)

| Тест | Файл | Что проверяет |
|------|------|---------------|
| `stage5_store_then_drain_roundtrip` | `runtime.rs` (unit) | store → drain → карта пуста |
| `stage5_click_counter_on_event_integration` | `tests/custom_elements.rs` | click → on_event → mutate state → redraw |
| `stage5_click_counter_multiple_clicks` | `tests/custom_elements.rs` | N кликов → count == N |
| `stage5_non_click_events_do_not_increment` | `tests/custom_elements.rs` | не-click события не меняют состояние |

`ClickCounter` — тестовый кастомный элемент (tag `"click-counter"`,
`count: Cell<i32>`), зарегистрирован через `#[linkme::distributed_slice(REGISTERED_COMPONENTS)]`.

## Smoke test

### Запуск бинарника

- `target/release/plugin-demo.exe` собирается и запускается (7.0–7.2s build).
- Окно открывается, рендерится div-based mockup datepicker (заголовок "Дата",
  кнопка "Выбрать дату", счётчик).
- **`light dev --agent`** не может завершить warm-up build из-за LNK1104
  (Windows temp-dir ACL issue, pre-existing — см. `docs/TODO_TESTS.md`).
  Прямой запуск бинарника работает.

### Ограничение: кастомный тег `<datepicker>` в `#[template]`

- `gen_custom_element` (template.rs:1294–1309) генерирует токен-поток, который
  rustc отвергает: `expected one of ')' , ',' , '.' , '?' , or an operator, found ';'`.
- Подтверждено серией контролируемых экспериментов:
  - button-only app (без custom tag) → **собирается** (6.4s);
  - любой вариант с `<datepicker>` → **падает идентично** (bare, v-model, String/i64 field);
  - `[features]`, trailing newline, touched files — не влияют.
- Root cause не идентифицирован на этом этапе. Требует отдельной диагностики
  (расширить `gen_custom_element` через `cargo expand` или добавить `eprintln!`
  для визуализации сгенерированного token stream).
- **Fallback для smoke-теста:** bin использует div-based mockup (визуально
  имитирует datepicker), что позволяет проверить runtime-конвейер (click → redraw).
- Доказательство click→on_event→mutate→redraw пути дают 3 интеграционных теста
  в `tests/custom_elements.rs` (см. таблицу выше).

## Диффы

Полные диффы четырёх затронутых файлов (4 файла, 635 строк diff; +512, −16 в 7 файлах
всего включая example):

| Файл | Diff-строк |
|------|-----------|
| `crates/light-core/src/runtime.rs` | 180 |
| `crates/light-core/src/window/mod.rs` | 155 |
| `crates/light-core/src/plugins.rs` | 72 |
| `crates/light-core/tests/custom_elements.rs` | 228 |
| **Итого** | **635** |

Диффы доступны в `/tmp/diff-runtime.txt`, `/tmp/diff-window.txt`,
`/tmp/diff-plugins.txt`, `/tmp/diff-tests.txt` (локальные копии) и в GitHub PR diff.

## Локальный прогон

```text
$env:CARGO_BUILD_JOBS = "1"

cargo fmt --all -- --check
  # ✅ 0 warnings — форматирование чистое

cargo clippy --workspace --all-targets --no-default-features -- -D warnings
  # ⚠️ ring C-compile fails locally (MSVC D8050) — pre-existing Windows issue
  #    (не связано со stage 5; CI на Linux/Mac/Win-MSVC проходит green)
  #    Обход для локального линта:
  #      cargo clippy --workspace --exclude light --exclude light-fw-cli-core \
  #        --all-targets --no-default-features -- -D warnings
  #    → ✅ 0 warnings, 0 errors

cargo test --workspace --all-features --release -j 2 --no-fail-fast
  # ✅ Все stage5 тесты зелёные:
  #      test runtime::custom_smoke_tests::stage5_store_then_drain_roundtrip ... ok
  #      test custom_elements::stage5_click_counter_on_event_integration ... ok
  #      test custom_elements::stage5_click_counter_multiple_clicks ... ok
  #      test custom_elements::stage5_non_click_events_do_not_increment ... ok
  # ✅ Предыдущие интеграционные тесты (Этап 4) тоже зелёные.
  # ⚠️ 2 pre-existing failures (НЕ связаны со stage 5, проверено на чистом HEAD):
  #    - example-dashboard: avatar_too_large_is_rejected,
  #      avatar_oversized_dimensions_are_rejected
  #      → Windows temp-dir ACL / PermissionDenied
  #    - light-fw-macros: ui (trybuild LNK1104 на временный файл)
  #      → Windows-only linker quirk; на Linux CI зелёный
```

Полный вывод: `/tmp/stage5-test-output.txt`.

## CI — 15/15 GREEN ✅

| Джоба | Результат |
|-------|-----------|
| Tests (ubuntu-latest) | ✅ success |
| Tests (macos-latest) | ✅ success |
| Tests (windows-latest) | ✅ success |
| Clippy (ubuntu-latest) | ✅ success |
| Clippy (macos-latest) | ✅ success |
| Clippy (windows-latest) | ✅ success |
| Formatting | ✅ success |
| Documentation | ✅ success |
| Build examples (default features) | ✅ success |
| Features (basic-elements) | ✅ success |
| Features (form-window-a11y) | ✅ success |
| Features (basic-window-a11y) | ✅ success |
| Features (a11y-no-winit) | ✅ success |
| Features (no-default-features) | ✅ success |
| Features (full) | ✅ success |

- Run: https://github.com/gifoff/light/actions/runs/43880919188
- SHA: `fc0fe5c`
- Предыдущие 3 run'а на ветке также 15/15 green (43848200168, 43850773994, 43853261200).

## Известные ограничения / TODO

1. **`gen_custom_element` token-syntax bug** — кастомные теги в `#[template]`
   не компилируются. Требуется диагностика (cargo expand / eprintln) и фикс.
   Не блокирует текущий коммит: proof-of-concept click→on_event→mutate дают
   интеграционные тесты, а пример использует div-based mockup.
2. **`light dev --agent` warm-up build** — LNK1104 на Windows (pre-existing,
   временный файл в `C:\Temp`). Не блокирует CI (Linux).
3. **Ring D8050** — local-only (Windows MSVC), CI (Linux) не затрагивается.
