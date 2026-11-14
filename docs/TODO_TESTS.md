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
