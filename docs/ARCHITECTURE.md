# Архитектура Light

Документ для новых контрибьюторов: как устроен фреймворк, где проходят границы
между крейтами и что происходит от HTML-шаблона до кадра в окне.

## Крейты воркспейса

| Крейт | Роль |
| --- | --- |
| `light-fw-core` | Ядро: виртуальный DOM, реактивность, layout, paint, окно, a11y, валидация. |
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
- **`window`** — цикл `winit`, управление окнами. Подмодуль `devtools` (фича `devtools`) — TCP-сервер на 127.0.0.1:9229.
- **`window::paint`** — программная растеризация.
- **`a11y`** — построение дерева AccessKit.
- **`validation`** — `required`, пользовательские валидаторы.

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
light-gui          # окно winit, события, a11y (из window/, control.rs, a11y/)
light-render-cpu   # программная отрисовка (из render/softbuffer.rs, window/paint.rs)
light-render-gpu   # аппаратная отрисовка (из render/wgpu.rs)
light-fw           # фасад (re-export, run(), run_with_config)
```

Этапы:
1. **Скелет** (этап 1, этот коммит): 5 пустых крейтов + связи + workspace.
2. **Перенос**: реальные переезды по одному слою за раз, каждый со своим
   smoke-тестом и CI.
3. **Фасад**: `light-fw` переключается на новые слои, `light-fw-core`
   остаётся совместимым (re-export) до удаления.
