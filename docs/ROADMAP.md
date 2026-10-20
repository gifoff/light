# Roadmap Relay

## Версия 0.2

- CLI: `check`, `suggest`, `doctor`, `add`, `remove`, `update`, `lock`, `vendor`.
- `relay.lock` и воспроизводимые сборки.
- Компонентная модель: `relay-core`, `relay-element-form`, `relay-behavior-validation`.

## Версия 0.3 (текущая)

- Публикация компонентов в crates.io ✓.
- `relay publish` ✓.
- `relay init` ✓.
- `docs/COMPONENT_AUTHORING.md` ✓.
- Добавить `repository` в Cargo.toml компонентов ✓.

## Версия 0.4 (выполнено)

- **CPU-рендеринг** как альтернатива wgpu ✓ (done).
  - Было: `pixels` → `wgpu` → `naga` + `wgpu_core` + `wgpu_hal` = ~3.8 MB
    (49% бинарника).
  - Сделано: программная растеризация уже была, показ кадра переведён на
    `softbuffer` (`SoftbufferBackend` под фичей `softbuffer-backend`,
    включена по умолчанию). `wgpu` остался как `wgpu-backend` для сравнения.
  - Результат: release-сборка `relay-demo` — с 7.86 MB до 1.70 MB
    (−6.16 MB, −78%).
  - Риск HiDPI/производительности не подтвердился: кадр рисуется в том же
    RGBA-буфере, размеры берутся в физических пикселях.

## Не в плане

- Замена Rust на другой язык.
- Свой менеджер пакетов вместо Cargo.
- Свой реестр вместо crates.io.
