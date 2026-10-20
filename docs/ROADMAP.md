# Roadmap Light

## Версия 0.2

- CLI: `check`, `suggest`, `doctor`, `add`, `remove`, `update`, `lock`, `vendor`.
- `light.lock` и воспроизводимые сборки.
- Компонентная модель: `light-fw-core`, `light-fw-element-form`, `light-fw-behavior-validation`.

## Версия 0.3 (текущая)

- Публикация компонентов в crates.io ✓.
- `light publish` ✓.
- `light init` ✓.
- `docs/COMPONENT_AUTHORING.md` ✓.
- Добавить `repository` в Cargo.toml компонентов ✓.

## Версия 0.4 (выполнено)

- **CPU-рендеринг** как альтернатива wgpu ✓ (done).
  - Было: `pixels` → `wgpu` → `naga` + `wgpu_core` + `wgpu_hal` = ~3.8 MB
    (49% бинарника).
  - Сделано: программная растеризация уже была, показ кадра переведён на
    `softbuffer` (`SoftbufferBackend` под фичей `softbuffer-backend`,
    включена по умолчанию). `wgpu` остался как `wgpu-backend` для сравнения.
  - Результат: release-сборка `light-demo` — с 7.86 MB до 1.70 MB
    (−6.16 MB, −78%).
  - Риск HiDPI/производительности не подтвердился: кадр рисуется в том же
    RGBA-буфере, размеры берутся в физических пикселях.

## Версия 0.5 (выполнено)

- `light init --with form|validation` ✓.
- `light add` для сторонних `light-fw-*` крейтов с crates.io и флагом
  `--features` ✓.
- `light search <запрос>` для поиска компонентов на crates.io ✓.
- Автоматическое включение фич мета-крейта `light` при добавлении компонента ✓.
- `light remove` и `light.lock` для внешних компонентов ✓.
- `docs/EXTERNAL_COMPONENTS.md` ✓.
- Очистка хвостов старого имени и временных путей в тестах ✓.

## Версия 0.6 (выполнено)

- `light init --template counter|todo|calculator` с примерами через
  `include_str!` ✓.
- Каталог `examples/` с showcase-приложениями ✓.
- Кросс-платформенный CI (Linux, Windows, macOS) ✓.
- `docs/ARCHITECTURE.md` для контрибьюторов ✓.
- Исправлена обработка многобайтовых символов в HTML-парсере ✓.

## Не в плане

- Замена Rust на другой язык.
- Свой менеджер пакетов вместо Cargo.
- Свой реестр вместо crates.io.
