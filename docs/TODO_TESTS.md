# Известные проблемы с тестами

## macOS: `measure_excludes_line_gap` — латентный дефект, раскрыт в 880a48d (2026-11-27, OPEN)

**Симптом:** CI run 46032897413 на macos-latest красный на одном тесте
(`window::font::tests::measure_excludes_line_gap`, `font.rs:357`):
`assertion failed: (scaled.height() - height - scaled.line_gap()).abs() < 0.01`.
Всё остальное зелёное (light-gui lib 269 passed; 7 paint-тестов + canary — green).

**Статус: латентный, НЕ pre-existing.** Корректная классификация: до `880a48d`
(включение macOS-путей в `FontMetrics::load`) тест **молча пропускался** на
macOS через early-return `else { return; }` в `FontMetrics::load() -> Err`.
То есть «падал» он только после того, как мы починили загрузку шрифтов и он
начал реально выполняться. Формулировка «pre-existing failure» неверна и
отозвана: это наш дефект, замаскированный silent-skip'ом, а не проблема
окружения или платформы. (См. FIX6b-отчёт: `target/temp/FIX6-REPORT.md`.)

**Причина (диагностика, чтение исходников ab_glyph 0.2.32 + ttf-parser):**
старый assertion строка 357 `(scaled.height() - height - scaled.line_gap())`
вырождается в `|line_gap| < 0.01`, потому что `ab_glyph::Font::height()` по
определению равен `ascent - descent` (и никогда не включает `line_gap`)
(`ab_glyph-0.2.32/src/scale.rs:118–127`, `src/font.rs:90–95`), а `height`
(второе значение `measure()`) = `ascent - descent` (`font.rs:320`). На Windows
(SEgoe UI) `line_gap == 0` → проверка тривиально проходила; на macOS загруженный
шрифт имеет ненулевой `line_gap` → проверка падала. Это проверка конкретного
загруженного шрифта, а не контракта `measure()`.

**Починено (тест переписан под реальный контракт, не ослаблен):**
`test(gui): assert real measure() contract instead of font-specific algebra`
(commit `5cf9e12`). Новый тест независимо (не через `measure()`) вычисляет
ожидаемую высоту как `scaled.ascent() - scaled.descent()` и ожидаемую ширину
как сумму `h_advance` по символам основного шрифта, и сравнивает их с фактическим
результатом `measure()`. Без tautology, без числового identity на `line_gap`.
Локально (Windows): 81 passed / 0 failed, включая все 7 paint-тестов + canary.
Окончательное подтверждение на macOS придёт со следующим CI-run'ом.

## Открытые items из FIX6/FIX6b (2026-11-27)

- **OPEN — молчаливый skip в `font.rs:349` (закрыто коммитом `932beb2`/`5cf9e12`).**
  Все 4 font-dependent теста, которые молчаливо возвращались при недоступности
  системного шрифта, теперь печатают явную строку `SKIP: …` (с названием ОС)
  перед `return`. Места: `font.rs` `measure_excludes_line_gap` (в рамках
  `5cf9e12`), `font.rs` `whitespace_uses_primary_font` (в рамках `5cf9e12`),
  `menu.rs` `text_fits_vertical_line_box` + `ellipsized_text_fits_width`
  (в рамках `932beb2`). Тихие скипы запрещены инвариантом FIX6b.

- **Наблюдение (НЕ установленный баг) — Windows-лоадер подтягивает emoji/symbol-шрифты.**
  При диагностике измерений локально (Windows, Segoe UI) выяснилось, что
  `FontMetrics::load()` на Windows дополнительно подтягивает эмодзи/символьные
  fallback-шрифты (2 fallback'а в `fonts[]`). Это ожидаемое поведение
  (CANDIDATES + EMOJI_CANDIDATES), не баг — зафиксировано как наблюдение для
  осведомлённости, приоритет низкий.

- **OPEN — ticket «bundle font», `app.rs:81` (`Error::Font`).**
  Решение об бандлинге шрифта в крейт (вместо зависимости от системного)
  требует архитектурного решения: оставить ошибку `Error::Font` как есть,
  либо добавить bundled-шрифт как fallback. Пока architect не примет решение —
  item открыт, production-код не меняется (scope-gate FIX6).

## macOS font-discovery fix: `880a48d` (2026-11-27, закрывает root-cause CI-red)

`crates/light-gui/src/window/font.rs` — `FontMetrics::load()` теперь включает
macOS-пути (`/System/Library/Fonts`, `/System/Library/Fonts/Supplemental`) и
соответствующие кандидаты в списки поиска, поэтому на macos-latest системный
шрифт успешно закладывается. Это сняло root-cause красноты CI 46032897413.
Отдельно от этого фикса тест `measure_excludes_line_gap` имел собственную
латентную проблему (см. раздел выше) — она исправлена в `5cf9e12`.

## `light devtools move` / `move-leave` не работают с runtime-templates (known issue, цель — 0.22.0) (2026-11-24)

**Симптом:** при включённой фиче `runtime-templates` команды
`light devtools move --key <k>` / `move-leave` возвращают `{ok:false}`
для узлов с `@mouseenter`/`@mouseleave`, хотя обработчик в шаблоне есть
и `light devtools state` показывает на узле `data-event="Mouseenter:…"`.

**Причина (диагностика, чтение исходников):** с `runtime-templates`
маппинг `@mouseenter`/`@mouseleave` живёт только в runtime-дереве
(`light-vdom/src/runtime/parse.rs:445–474` пишет `data-event`), а
DevTools-команды диспетчируют событие через `component.dispatch`
(`light-gui/src/window/devtools.rs:445–477`) → статические `#arms` +
`dispatch_by_name` + `dispatch_vmodel` — без fallback'а на `invoke_handler`
(`light-macros/src/template/methods.rs:82–127`), который реально работает
с runtime-деревом. Реальное наведение мышью (окно) идёт через
`WindowCommand::InvokeHandler` → `invoke_handler` и работает корректно —
это ограничение именно DevTools-инструмента, не фреймворка.

**Обход для E2E/тестов:** программный вызов обработчика через кнопку
`@click` (`light devtools click --key …`), либо ручное наведение.

**Предлагаемое исправление (0.22.0):**
1. В `crates/light-gui/src/window/devtools.rs` для `MoveEnter`/`MoveLeave`
   добавить после `dispatch_custom_event`:
   `|| window.component.invoke_handler(&key, event.clone())`.
2. Либо в `pipeline.rs::dispatch` под `FEATURE_RUNTIME_TEMPLATES` вызывать
   `invoke_handler` как fallback после `dispatch_by_name` / `dispatch_vmodel`.
3. Минимум — диагностическое сообщение вместо голого `ok:false`
   («ключ X не найден в статическом дереве; посмотрите `state` для
   runtime-атрибутов `data-event`»).

Приоритет: средний. Задокументировано в `docs/guide.md` (примечание
в разделе CLI-обёртка) и в CHANGELOG `[Unreleased]` / Known limitations.
Найден в ходе E2E 0.21.0 (см. `C:\dev\test-workspace\final-e2e\REPORT-E2E-0.21.0.md`).

## `light devtools find` не ищет runtime-дерево (known issue, цель — 0.22.0) (2026-11-24)

`find --text "…"` возвращает `null` для элементов, созданных динамически
(`v-if`/`<template>`) при включённой `runtime-templates`: поиск идёт по
статическому (до-рендер) дереву. Обход: `light devtools tree` / `state`
для просмотра актуального VDOM и явные `key` для кликов. Приоритет: низкий.

## Pre-existing CI failures, починены в release/0.21.0 (2026-11-24)

- **Clippy `io_other_error`** — `crates/light-gui/src/devtools/server.rs:89`:
  `std::io::Error::new(ErrorKind::Other, …)` → `std::io::Error::other(…)`.
  Clippy `-D warnings` (rust 1.100.0) падает на всех 3 OS. Фикс: `16b7dd0`.
- **Image-test sandbox** — `crates/light-vdom/src/runtime/tests.rs:489`:
  `create_dir_all` / `fs::write` → os error 5 (PermissionDenied) на macOS CI
  и локально под DSH sandbox. Фикс: skip-guard — при недоступности ФС тест
  логирует `note: skipping` и завершается без panic (`16b7dd0`, `fb1f825`).
- **Clippy `derivable_impls`** — `crates/light-ui/src/widgets/toast/mod.rs:72`:
  ручной `impl Default for ToastManager` → `#[derive(Default)]`. Стало жёсткой
  ошибкой только после того, как все upstream-крейты закомпилились чисто.
  Фикс: `4beaf87`.

## Pre-existing CI failures в light-cli-core: версия 0.21.0-alpha.1 vs захардкоженное 0.20 (2026-11-26)

**Симптом:** 3 теста в `crates/light-cli-core` падают на всех 3 OS (Ubuntu, macOS, Windows):

- `init::tests::tests::apply_creates_project_and_lockfile`
  (`src/init/tests.rs:361`) — `assert_eq!(lock.core, expected_core)` где
  `expected_core = format!("{}.0", minor_version())`, а фактический `lock.core`
  содержит pre-release метку ядра (`0.21.0-alpha.1`).

- `metadata::tests::tests::validate_ok_for_compatible_component`
  (`src/metadata/tests.rs:188`) — манифест с `core = "^0.20"` ожидает `Ok`, но
  `LIGHT_CORE_VERSION = "0.21.0-alpha.1"` → semver `^0.20` не удовлетворяет
  `0.21.0-alpha.1` (по формальному semver pre-release ниже release).

- `metadata::tests::tests::validate_empty_provides_is_warning_not_fatal`
  (`src/metadata/tests.rs:246`) — аналогично: `core = "^0.20"` → IncompatibleCore
  вместо ожидаемого Warning(EmptyProvides).

**Причина:** те же 3 теста были красными **до** фикса `a090a2e`
(runs 45796590578 от 09:56, 45780448198 от 08:06). Fails в крейте, который **не
затрагивался** FIX1 (7 файлов: CHANGELOG.md, light-ui×5, light-vdom — ни одного
ref'а на light-cli-core). Корневая причина: bump версий до `0.21.0-alpha.1`
(commit `897e032`) не обновил захардкоженные `"0.20"`/`"0.20.0"` в тестах
cli-core и логику `validate_metadata`, которая использует строгий `semver::VersionReq`
(там pre-release имеет семантику «ниже release», в отличие от loose-matcher'а в
`light-vdom` parse_triple).

**Предлагаемое исправление (отдельный тикет, НЕ часть FIX1):**
1. В `crates/light-cli-core/src/metadata/` — заменить `semver::VersionReq`
   сравнение на ту же loose-логику, что в `light-vdom` (parse_triple + minor-floor),
   ИЛИ явно игнорировать pre-release в `validate_metadata`.
2. В `crates/light-cli-core/src/init/tests.rs:361` — учесть pre-release в expected.
3. Проверить рассинхрон версий cli-core vs ядра (Cargo.toml).

**Приоритет:** высокий — блокирует зелёный CI на release/0.21.0.

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