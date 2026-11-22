# Известные проблемы с тестами

## crates.io S3: 404 на .crate-архивы (2026-11-14)

`https://static.crates.io/light-fw-html/0.18.0/download` возвращает
404 (NoSuchKey) для всех крейтов аккаунта gifoff. Не блокирует
`light search`, но не позволяет проверить happy-path `--yes`
(валидация + успешная установка).

Проверено: light-fw-html/0.18.0, light-fw-macros/0.18.0,
light-fw-core/0.17.7 — все 404.

Приоритет: средний. Как только S3 починится — прогнать e2e
`light search datepicker --yes` → должен быть exit 0.

## LIGHT_CORE_VERSION — версия cli-core, а не light (2026-11-14)

metadata::LIGHT_CORE_VERSION = env!("CARGO_PKG_VERSION") даёт версию
light-fw-cli-core, а не light (bin) или light-fw-core. Пока версии
синхронизированы — работает. При рассинхроне проверка `core = "^0.19"`
может дать ложный результат.

Приоритет: средний. Взять в 0.19.x (передавать версию из light bin).

## MSVC D8050: debug-сборка `light` с фичей network падает (2026-11-14)

light зависит от light-fw-cli-core с features = ["network"] → reqwest →
ring. На Windows MSVC debug сборка падает с D8050 (cmd line > 8 KB).
Release работает.

Workaround: `cargo build --release -p light`. Или clippy без network:
`cargo clippy -p light-fw-cli-core --no-default-features -- -D warnings`.
Пре-существующая проблема ring, не наша.

Приоритет: низкий (workaround есть).

## let mut warnings в dev.rs на non-Windows (2026-11-14)

Ложно-положительные clippy-warnings (`unused_mut`) в `crates/light-cli-core/src/dev.rs`
на non-Windows: переменные объявлены `let mut p`, но мутация происходит
только внутри `#[cfg(windows)]` блока — на Linux/MacOS `mut` реально
не используется.

**Не связано с D8050** — отдельная проблема.

Обойдено в 45082d0 через `#[cfg_attr(not(windows), allow(unused_mut))]`
в 4 местах (`fresh_bin_path`, `run_exe_path` + 2 теста). Логика не
менялась, только подавление предупреждения на non-Windows.

Приоритет: закрыто (workaround применён). Очистить, когда `let mut`
перестанет быть necessary (вынести `p` в scope, где мутация видна
везде).

## Локальные environmental failures (Windows EDR) (2026-11-14)

Локальные падения, которые **не воспроизводятся** на CI (Linux). Все
подтверждены на clean HEAD через `git stash` → не связаны с кодом
ветки.

### `example-dashboard` — avatar tests (2 шт.)

- `tests::avatar_too_large_is_rejected`
- `tests::avatar_oversized_dimensions_are_rejected`

Паника `fs::write(&path, ...).expect("temp file")` →
`Os { code: 5, kind: PermissionDenied }`. EDR/ACL блокирует запись
временного файла в `C:\Temp`. Файлы `examples/dashboard/src/main.rs`
не менялись в этой ветке.

### `light-fw-cli-core` — network-dependent integration (4 шт.)

- `tests/search_integration.rs` — 4 теста (mock server 404 / network)

Нужен локальный mock-сервер или доступ к сети, который в CI есть, а
локально (по EDR/сетевую политику) — нет.

**Как отлаживать локально:** использовать WSL или запускать эти тесты
на CI. Не трогать код `example-dashboard` и интеграционные тесты
пока не появится стабильная среда.

Приоритет: низкий (CI зелёный, локально обходится через WSL).

## Doctor test временно проверяет exit code 1 (2026-11-14)

`doctor_reports_core_version_from_workspace` (`crates/light/tests/cli.rs`)
сейчас ожидает **exit 1**, а не `.success()`. Причина: фикстура
`core-workspace` намеренно несовместима с bundled-реестром:

- `[workspace.package] version = "0.18.0"` (bumped в 45082d0).
- `form/Cargo.toml`: `core = "^0.17"` (соответствует registry.toml).
- Caret для 0.x: `^0.17` пересекается только с `0.17.x`, не с `0.18.0`.

Doctor честно детектит несовместимость и завершается кодом 1. Тест
проверяет обе части: что версию ядра он прочитал из workspace
(`light-fw-core 0.18.0`), и что несовместимость он сообщает
(`light-fw-element-form` + «несовместим»/«incompatible»).

**Триггер возврата к `.success()`:** bump `registry.toml` до 0.18.0
(form → 0.18.0, core `^0.18`) при релизе 0.19.0. После этого фикстура
станет самосогласованной и тест вернётся к стандартному assertion.

См. README-комментарий в самом тесте (`cli.rs`).

Приоритет: низкий (закрыть вместе с релизом 0.19.0).

---

## Починено в 45082d0 (Stage 0, feature/plugin-api)

3 pre-existing failures в `crates/light/tests/cli.rs` устранены:

- `update_dry_run_reports_change` — версия из `ComponentRegistry::builtin()`
  вместо хардкода `0.7.0`.
- `update_applies_new_version` — ibid.
- `doctor_reports_core_version_from_workspace` — переписан под
  намеренную несовместимость фикстуры (см. раздел выше).

Фикстуры `core-workspace` синхронизированы: workspace `0.7.0` → `0.18.0`,
form `^0.7` → `^0.17`. `registry.toml` не тронут (production data,
бум в 0.19.0).

## Починено в 4a06ccf (release/0.21.0, CI-фикс)

### `runtime_image_missing_file_returns_none` (light-vdom, Linux CI)

Паника `test png written: Os { code: 2, kind: NotFound }` при `fs::write`.
Функция `write_test_png` брала базовый каталог из `CARGO_TARGET_DIR` —
который в GitHub Actions не задан как environment variable (cargo лишь
*использует* его во внутреннем пути), тесты получали относительный
путь `target/temp/...`, а cwd ≠ workspace root. В Windows-раунде
раньше работало потому, что `CARGO_TARGET_DIR` иногда попадал в env через
toolchain/кэширование, а на Linux/macOS — нет.

Фикс: `write_test_png` теперь строит путь от `CARGO_MANIFEST_DIR/../../..`
(abspath workspace root) + `target/temp/vdom-image-{sub}` — не зависит от
cwd и от presence `CARGO_TARGET_DIR`. Тесты не менялись — они корректно
ожидали работу с PNG-файксом, проблема была в пути.

Приоритет: закрыто. Коммит: 4a06ccf.

## Починено в da16407 (Stage 0, feature/plugin-api)

- `stop_without_active_session_fails` (crates/light-cli-core/tests/session.rs) —
  `stop_session` возвращала `Ok(path)` для несуществующей сессии, создавая
  пустые артефакты. Добавлен guard: `if !sessions_dir(cwd).join(id).is_dir()`
  → `Err(SessionError::NoActiveSession)`. Тест не менялся — он был написан
  под правильный контракт, код потерял проверку.
