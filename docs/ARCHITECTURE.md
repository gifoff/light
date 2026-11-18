# Архитектура Light

Документ для новых контрибьюторов: как устроен фреймворк, где проходят границы
между крейтами и что происходит от HTML-шаблона до кадра в окне.

## Крейты воркспейса

| Крейт | Роль |
| --- | --- |
| `light-fw-core` | Ядро: валидация, фасад 0.19.x. GUI-слой (окно, paint, a11y) вынесен в `light-gui` в 0.20.0. |
| `light-gui` | GUI-слой: окно на `winit`, события ввода, а11y-мост AccessKit, рендер-бэкенды (`softbuffer`/`pixels`), асинхронный рантайм, DevTools. Добавлен в 0.20.0 (Этап 4). |
| `light-fw-html` | Парсер HTML-шаблонов: теги, атрибуты, директивы, `{{ … }}`, `<include>`. |
| `light-fw-macros` | Процедурные макросы `#[template]` и `#[proxy]`. |
| `light-fw` | Мета-крейт: реэкспортирует ядро, макросы и подключённые компоненты. |
| `light-fw-element-form` | Компонент-элемент: `<checkbox>`, `<radio>`, `<select>`, `<option>`. |
| `light-fw-behavior-validation` | Компонент-поведение: `required`, `:validator`, `@validate`. |
| `light` | Бинарник `light`: команды `build`, `dev`, `check`, `add`, `search`, `publish`. |
| `light-fw-cli-core` | Логика CLI без привязки к терминалу: анализ, план, применение, lock, hot-reload supervisor. |
| `light-fw-devtools-web` | Internal: HTTP-сервер Web UI DevTools. Публикуется как зависимость `light`, не позиционируется как пользовательский компонент. |
| `light-demo` | Демонстрационное приложение и примеры. |

Зависимости направлены внутрь: компоненты и CLI зависят от ядра, ядро не знает
о прикладном коде.

## Компиляция шаблона

1. `#[template("templates/app.html")]` читает файл относительно `CARGO_MANIFEST_DIR`.
2. `light-fw-html` разбирает разметку в дерево `Node`/`Element`/`Attr`.
3. `light-fw-macros` обходит дерево, разрешает `<include src="...">`, проверяет директивы и генерирует Rust-код.
4. Сгенерированный код реализует `Component::render`.
5. `#[proxy]` добавляет реактивные геттеры/сеттеры и `new()`.

Ошибки разметки и неизвестные атрибуты становятся ошибками компиляции с указанием файла и строки.

## Рантайм: от состояния до кадра

```text
Component::render() → VElement (новое дерево)
        │
        ├── сравнение с предыдущим деревом (vdom::diff)
        ▼
   набор операций (Op) → Dom → layout (размеры и позиции) → paint (Canvas → RGBA) → RendererBackend::present()
```

Основные слои `light-fw-core`:

- **`vdom`** — `VElement`, `NodeKind`, стили, события и `diff`. Ключи узлов стабильны между отрисовками.
- **`reactive`** — свойства `Proxy`, наблюдатели и запрос перерисовки.
- **`layout`** — вычисление `Rect` для каждого узла.
- **`light-gui::window`** — цикл `winit`, управление окнами. Подмодуль `devtools` (фича `devtools`) — TCP-сервер на 127.0.0.1:9229.
- **`light-gui::window::paint`** — программная растеризация.
- **`light-gui::a11y`** — построение дерева AccessKit.
- **`light-fw-core::validation`** — `required`, пользовательские валидаторы.

Публичный API 0.19.x сохранён через реэкспорт: `light_fw_core::window`,
`light_fw_core::a11y`, `light_fw_core::async_runtime` и т.д. указывают на
соответствующие модули `light-gui`.

События приходят из `winit`, преобразуются в `vdom::Event` и передаются компоненту.

## CLI и Hot Reload

`light` (bin) — тонкий слой `clap` над `light-fw-cli-core`. 

Команда `light dev` (Hot Reload) работает как supervisor:
- **Auto-detect**: если запущена без аргументов, сканирует текущую директорию и находит пакет.
- **Warm-up build**: собирает проект в `target/hot-build`.
- **Watcher**: отслеживает изменения в `src/` и `templates/`.
- **Template Reload**: при изменении `.html` отправляет сигнал `RELOAD_TEMPLATE` по DevTools, без пересборки.
- **Binary Swap**: при изменении `.rs` пересобирает, отправляет `SAVE_ALL`, убивает старый процесс и запускает новый с `LIGHT_RESUME_FROM_SWAP=1`.

Конфигурация проекта (включая запуск DevTools) читается из `light.config.json` через `light_fw::run_with_config()`.

## Целевая структура 0.20.0 (Декомпозиция `light-fw-core`)

Философия: ядро минимально. VDOM и HTML-парсер НЕ лежат в одном крейте
с реактивностью и графикой. Если пользователь делает API-сервер, он не
должен тянуть VDOM.

```
light-reactivity   # сигналы, состояние (из light-core/src/reactive/)
light-html         # парсер HTML (уже существует, light-fw-html)
light-vdom         # виртуальный DOM, layout, diff (из vdom.rs, layout.rs, dom.rs)
light-gui          # окно winit, события, a11y, рендер-бэкенды, async-runtime (из window/, render/, a11y/, async_runtime.rs, devtools/, file_dialog.rs)
light-render-cpu   # программная отрисовка (из render/softbuffer.rs, window/paint.rs)
light-render-gpu   # аппаратная отрисовка (из render/wgpu.rs)
light-fw           # фасад (re-export, run(), run_with_config)
```

Зависимости направлены внутрь: `light-fw-core → {light-gui, light-vdom, light-reactivity, …}`,
`light-gui → {light-vdom, light-reactivity, base64, thiserror, serde, serde_json}` + опциональные
(winit, ab_glyph, png, rfd, softbuffer, pixels, futures, accesskit).
`light-vdom` не имеет прямой зависимости от `light-fw-core`.

Этапы:
1. **Скелет** (этап 1): 5 пустых крейтов + связи + workspace. ✅
2. **Перенос VDOM** (этап 2–3): `vdom.rs`, `layout.rs`, `dom.rs`, `text_wrap.rs` → `light-vdom`;
   `window_backend` (канал `WindowCommand`/`WindowToken`) тоже в `light-vdom`. ✅
3. **Перенос GUI** (этап 4): `window/`, `render/`, `a11y/`, `devtools/`, `file_dialog.rs`,
   `async_runtime.rs` → `light-gui`. Публичный API 0.19.x сохранён через реэкспорт.
   Headless-режим (`cargo check -p light-fw-core --no-default-features`) не тянет
   winit/softbuffer/pixels/rfd/accesskit/futures. ✅
4. **Фасад**: `light-fw` переключается на новые слои, `light-fw-core`
   остаётся совместимым (re-export) до удаления.
