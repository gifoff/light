# Архитектура Light

Документ для новых контрибьюторов: как устроен фреймворк, где проходят границы
между крейтами и что происходит от HTML-шаблона до кадра в окне.

## Крейты воркспейса

| Крейт | Роль |
| --- | --- |
| `light-fw-core` | Ядро: фасад 0.19.x + **headless by default** (0.20.0). Реэкспорт всех слоёв (`reactive`, `vdom`, `window`, `a11y`, `validation`). Без окон/рендера/a11y в default-фичах. |
| `light-reactivity` | Сигналы: `Property`, `Proxy`, `Observer`, `Watcher`, `watch`. Вынесен из `light-fw-core` (0.20.0, этап 2). |
| `light-vdom` | VDOM, layout, DOM, runtime, plugins, validation. Вынесен из `light-fw-core` (0.20.0, этапы 2–3). |
| `light-gui` | GUI-слой: окно на `winit`, a11y-мост AccessKit, `file_dialog`, `async_runtime`, devtools-инфраструктура, render-абстракция. Вынесен из `light-fw-core` (0.20.0, этап 4). |
| `light-render-cpu` | Программная отрисовка через `softbuffer` (CPU-бэкенд). Вынесен из `light-gui` (0.20.0, этап 5). |
| `light-render-gpu` | Аппаратная отрисовка через `pixels` (wgpu/naga). Вынесен из `light-gui` (0.20.0, этап 5). |
| `light-fw-html` | Парсер HTML-шаблонов: теги, атрибуты, директивы, `{{ … }}`, `<include>`. |
| `light-fw-macros` | Процедурные макросы `#[template]` и `#[proxy]`. |
| `light-fw` | Фасад **GUI-first**: реэкспортирует ядро, макросы и подключённые компоненты. Default-фичи: `elements-basic`, `window`, `softbuffer-backend`, `a11y-winit`. |
| `light-fw-element-form` | Компонент-элемент: `<checkbox>`, `<radio>`, `<select>`, `<option>`. |
| `light-fw-behavior-validation` | Компонент-поведение: `required`, `:validator`, `@validate`. |
| `light` | Бинарник `light`: команды `build`, `dev`, `check`, `add`, `search`, `publish`. |
| `light-fw-cli-core` | Логика CLI без привязки к терминалу: анализ, план, применение, lock, hot-reload supervisor. |
| `light-fw-devtools-web` | Internal: HTTP-сервер Web UI DevTools. Публикуется как зависимость `light`, не позиционируется как пользовательский компонент. |
| `light-demo` | Демонстрационное приложение и примеры. |

Зависимости направлены внутрь: компоненты и CLI зависят от ядра, ядро не знает
о прикладном коде. GUI-приложения идут через фасад `light-fw` (GUI-first), а
headless-код — напрямую через `light-fw-core` с `default-features = false`.

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

Основные слои `light-fw-core` (реэкспорт под прежними именами 0.19.x):

- **`reactive`** (`light-reactivity`) — свойства `Property`/`Proxy`, наблюдатели и запрос перерисовки.
- **`vdom`** (`light-vdom`) — `VElement`, `NodeKind`, стили, события и `diff`. Ключи узлов стабильны между отрисовками. `v-if="false"` в `build_children` **пропускает** узел, а не прерывает список: соседние элементы рендерятся независимо (тест `v_if_false_skips_siblings_stay_rendered`).
- **`layout`** (`light-vdom`) — вычисление `Rect` для каждого узла.
- **`window`** (`light-gui`) — цикл `winit`, управление окнами. Подмодуль `devtools` (фича `devtools`) — TCP-сервер на 127.0.0.1:9229.
- **`window::paint`** (`light-gui/src/window/paint/`) — программная растеризация (render.rs, canvas.rs, mod.rs).
- **`a11y`** (`light-gui`) — построение дерева AccessKit.
- **`validation`** — `required`, пользовательские валидаторы.

Публичный API 0.19.x сохранён через реэкспорт: `light_fw_core::window`,
`light_fw_core::a11y`, `light_fw_core::async_runtime` и т.д. указывают на
соответствующие модули `light-gui`/`light-render-*`.

**Headless by default (0.20.0):** default-фичи `light-fw-core` =
`["elements-basic"]`. В headless-сборке (`--no-default-features` или
`default-features = false` у потребителя) отсутствуют: `winit`,
`softbuffer`/`pixels`/`wgpu`, `accesskit*`, `rfd`, `futures` (async).
Дерево зависимостей E2E-тестом подтверждено чистым (см. `docs/guide.md`, раздел
«API/CLI без GUI»).

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
light-gui          # окно winit, события, a11y, async-runtime (из window/, a11y/, async_runtime.rs, devtools/, file_dialog.rs)
light-render-cpu   # программная отрисовка (из render/softbuffer.rs); paint-модуль остался в light-gui/src/window/paint/
light-render-gpu   # аппаратная отрисовка (из render/wgpu.rs)
light-fw           # фасад GUI-first: re-export, run(), run_with_config()
                   # (default-фичи: elements-basic, window, softbuffer-backend, a11y-winit)
```

Зависимости направлены внутрь: `light-fw-core → {light-gui, light-vdom, light-reactivity, …}`,
`light-gui → {light-vdom, light-reactivity, base64, thiserror, serde, serde_json}` + опциональные
(winit, ab_glyph, png, rfd, futures, accesskit). Рендер-бэкенды вынесены:
`light-render-cpu` (`softbuffer`, фича `window`), `light-render-gpu` (`pixels`/wgpu/naga, фича `wgpu`).
`light-vdom` не имеет прямой зависимости от `light-fw-core`.

**Ключевой принцип 0.20.0 — Headless by default:** `light-fw-core` по умолчанию
не тянет ни окон, ни рендера, ни a11y. GUI включается точечными фичами
(`window`, `softbuffer-backend`, `wgpu-backend`, `a11y-winit`), а фасад
`light-fw` остаётся GUI-first для `light init`/`light dev`.

Этапы:
1. **Скелет** (этап 1): 5 пустых крейтов + связи + workspace. ✅
2. **Перенос VDOM** (этап 2–3): `vdom.rs`, `layout.rs`, `dom.rs`, `text_wrap.rs` → `light-vdom`;
   `window_backend` (канал `WindowCommand`/`WindowToken`) тоже в `light-vdom`. ✅
3. **Перенос GUI** (этап 4): `window/`, `render/`, `a11y/`, `devtools/`, `file_dialog.rs`,
   `async_runtime.rs` → `light-gui`. Публичный API 0.19.x сохранён через реэкспорт.
   Headless-режим (`cargo check -p light-fw-core --no-default-features`) не тянет
   winit/softbuffer/pixels/rfd/accesskit/futures. ✅
4. **Перенос рендера** (этап 5): `render/softbuffer.rs` → `light-render-cpu`;
    `render/wgpu.rs` → `light-render-gpu`. Paint-модуль остался в `light-gui/src/window/paint/`. ✅
5. **Headless by default** (этап 6, 0.20.0): default-фичи `light-fw-core`
   сокращены до `["elements-basic"]`; breaking change в payload'ах
   `light_fw_core::error::Error` (String вместо типизированных `winit`/`pixels`/
   `softbuffer` ошибок). Fасад `light-fw` без изменений (GUI-first). E2E-тест
   headless-сборки подтверждает чистое дерево зависимостей. ✅
