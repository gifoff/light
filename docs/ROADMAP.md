# Roadmap Relay

## Версия 0.2 (текущая)

- CLI: `check`, `suggest`, `doctor`, `add`, `remove`, `update`, `lock`, `vendor`.
- `relay.lock` и воспроизводимые сборки.
- Компонентная модель: `relay-core`, `relay-element-form`, `relay-behavior-validation`.

## Версия 0.3 (следующая)

- Публикация компонентов в crates.io.
- `relay publish`.
- `relay init`.
- `docs/COMPONENT_AUTHORING.md` ✓.
- Добавить `repository` в Cargo.toml компонентов.

## Версия 0.4 (исследование)

- **CPU-рендеринг** как альтернатива wgpu.
  - Сейчас: `pixels` → `wgpu` → `naga` + `wgpu_core` + `wgpu_hal` = ~3.8 MB
    (49% бинарника).
  - Замена на `softbuffer` + CPU-растеризация (`tiny-skia` или свой).
  - Ожидаемый выигрыш: ~3 MB из 7.7 MB.
  - Риск: сложно, много работы, риск сломать HiDPI и производительность.
  - **Решение:** откладываем до появления реальных пользователей,
    которые жалуются на размер. Пока — 7.7 MB приемлемо (для сравнения:
    Tauri ≈ 10–15 MB, Electron ≈ 150+ MB).

## Не в плане

- Замена Rust на другой язык.
- Свой менеджер пакетов вместо Cargo.
- Свой реестр вместо crates.io.
