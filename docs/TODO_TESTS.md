# Известные проблемы с тестами

## crates/light/tests/cli.rs — 3 pre-existing failures (0.18.0)

Три теста ожидают версий из старых фикстур (`0.7.0`), но проект давно на
0.18.0. Не связаны с Web UI DevTools, обнаружены при прогоне
`cargo test --workspace` перед релизом 0.18.0.

Проблемные тесты:

- `update_dry_run_reports_change`
  Ждёт в stdout `→ 0.7.0`, но CLI видит workspace-версию 0.18.0.
- `update_applies_new_version`
  После `light update` ожидает в `Cargo.toml` проекта
  `light-fw-element-form = "0.7.0"`.
- `doctor_reports_core_version_from_workspace`
  Ожидает в выводе `light doctor` строку `light-fw-core 0.7.0`.

Причина: фикстуры в `crates/light/tests/fixtures/{outdated-project,core-workspace}`
содержат захардкоженные версии `0.7.0` / `0.1.0`. Нужна отдельная задача по их
синхронизации (и, возможно, по переводу на автогенерацию версий
из `env!("CARGO_PKG_VERSION")`).

Приоритет: низкий (не блокер). Взять в 0.19.x.

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

## MSVC D8050: cargo build -p light (debug) падает (2026-11-14)

light зависит от light-fw-cli-core с features = ["network"] → reqwest →
ring. На Windows MSVC debug сборка падает с D8050 (cmd line > 8 KB).
Release работает.

Workaround: cargo build --release -p light. Или CARGO_BUILD_JOBS=1
(не всегда помогает). Пре-существующая проблема ring, не наша.

Приоритет: низкий (workaround есть). Задокументировать в AGENTS.md
(следующий коммит).
