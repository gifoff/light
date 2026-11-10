# Архитектура Light

Документ для новых контрибьюторов: как устроен фреймворк, где проходят границы
между крейтами и что происходит от HTML-шаблона до кадра в окне.

## Крейты воркспейса

| Крейт | Роль |
| --- | --- |
| `light-fw-core` | Ядро: виртуальный DOM, реактивность, layout, paint, окно, a11y, валидация. |
| `light-fw-html` | Парсер HTML-шаблонов: теги, атрибуты, директивы, `{{ … }}`, `<include>`. |
| `light-fw-macros` | Процедурные макросы `#[template]` и `#[proxy]`. |
| `light` | Мета-крейт: реэкспортирует ядро, макросы и подключённые компоненты. |
| `light-fw-element-form` | Компонент-элемент: `<checkbox>`, `<radio>`, `<select>`, `<option>`. |
| `light-fw-behavior-validation` | Компонент-поведение: `required`, `:validator`, `@validate`. |
| `light` | Бинарник `light`: команды `build`, `dev`, `check`, `add`, `search`, `publish`. |
| `light-fw-cli-core` | Логика CLI без привязки к терминалу: анализ, план, применение, lock, hot-reload supervisor. |
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

Конфигурация проекта (включая запуск DevTools) читается из `light.config.json` через `light::run_with_config()`.