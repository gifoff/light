# CLI Light

`light` — основной интерфейс работы с проектами Light: инициализация,
разработка, сборка, управление компонентами, публикация.

## Установка

```bash
cargo install light
```

## Команды

### `light init ИМЯ [--path PATH] [--minimal] [--template NAME] [--with COMPONENT]`

Создаёт новый проект Light (`ИМЯ` — имя проекта и крейта). Флаг `--with`
подключает компоненты (форм, валидации), `--template` — готовый showcase-шаблон,
`--minimal` — только `Cargo.toml` и `src/main.rs`.

```bash
light init my-app                            # пустой проект
light init my-app --template counter         # шаблон counter
light init my-app --with form --with validation
```

Доступные showcase-шаблоны: `counter`, `todo`, `calculator`. Готовые примеры
`dashboard`, `devtools-demo`, `multi-window`, `async-counter` лежат в `examples/`
(запуск — через `light dev` или `cargo run -p example-…`).

### `light dev [ПАКЕТ] [--agent] [--verbose] [--clean] [--preserve-state] [--no-auto-install] [EXTRA...]`

Запускает приложение с горячей перезагрузкой (HTML + `.rs`) и сохранением
состояния через DevTools. Перед запуском автоматически проверяет наличие
недостающих компонентов.

- `--agent` — создаёт сессию для автономного тестирования UI через `light devtools`;
- `--verbose` — подробные логи (debounce, hash-dedup, notify events);
- `--clean` — полный сброс кэша dev-сборки перед стартом;
- `--preserve-state` — сохранять `state.json` между всеми запусками;
- `--no-auto-install` — не устанавливать недостающие компоненты автоматически;
- дополнительные аргументы (включая `--release`, `--`) передаются в `cargo run` как есть.

```bash
light dev              # hot-reload
light dev --agent      # + сессия агента
light dev --release    # debug-запуск в release-режиме
```

### `light build [ПУТЬ] [-r|--release] [--no-release] [-f FEATURES] [--no-default-features] [--no-auto-install] [EXTRA...]`

Собирает приложение в release-режиме (`cargo build --release`). Перед сборкой
автоматически проверяет недостающие компоненты. После сборки выводит путь к
бинарному файлу и его размер.

Флаги: `-r`/`--release` (по умолчанию), `--no-release` (debug-сборка),
`-f FEATURES` (включить фичи, через запятую), `--no-default-features`,
`--no-auto-install` (не устанавливать недостающие компоненты автоматически).

```bash
light build              # сборка в текущем каталоге
light build ./my-app     # сборка в другом каталоге
light build -f form      # с фичей "form"
light build --no-release # debug-сборка
```

Дополнительные аргументы после `--` передаются в `cargo build` как есть.

### `light add КОМПОНЕНТ [версия]`

Устанавливает компонент (например, `light-fw-element-form`) в проект:
добавляет зависимость в `Cargo.toml` и включается через мета-фичи.

```bash
light add form             # latest версия
light add form 0.3         # конкретная версия
```

### `light remove КОМПОНЕНТ`

Удаляет компонент из проекта.

### `light check [--strict] [--format text|json]`

Анализирует проект: сканирует шаблоны, сопоставляет с реестром компонентов,
отображает установленные/недостающие/неиспользуемые компоненты. Флаг
`--strict` возвращает код 1 при проблемах.

```bash
light check              # отчёт по умолчанию
light check --strict     # для CI
light check --format json
```

### `light update [--all]`

Обновляет зависимости Light до последних версий.

### `light doctor`

Диагностика окружения: версия cargo, наличие компонентов, проблемы.

### `light publish [ПАКЕТ]`

Публикует крейт на crates.io. Требует чистый `git status` и настроенный
`CARGO_REGISTRY_TOKEN`.

```bash
light publish                       # весь воркспейс
light publish light-fw-element-form # один крейт
light publish --yes --skip-gates    # без вопросов, без pre-flight гейтов
light publish --no-verify           # не запускать cargo verify (по умолчанию)
```

### `light search ПОДАЧА`

Поиск компонентов на crates.io по ключевым словам.

### `light vendor [ПУТЬ] [--dry-run] [--offline]`

Копирует исходники компонентов в локальный каталог (по умолчанию `vendor/`)
для офлайн-модификации. Флаг `--dry-run` показывает план без копирования,
`--offline` — режим без сети.

### `light lock`

Создаёт или обновляет файл `light.lock` с версиями компонентов.

### `light suggest`

Подсказывает, какие компоненты стоит добавить в проект (на основе шаблонов).

### `light devtools`

Инструменты для автономного тестирования UI (работает при запущенном
`light dev --agent`):

| Команда | Описание |
|---|---|
| `light devtools tree` | Дерево VDOM (ключи, теги, текст, дети) |
| `light devtools layout` | Плоская раскладка с rect x,y,w,h |
| `light devtools state` | JSON-состояние приложения |
| `light devtools find --tag button --text "+1"` | Поиск элемента → ключ + rect |
| `light devtools click --key 0.0.0` | Клик |
| `light devtools type --key 0.0.1 "Hello"` | Ввод текста |
| `light devtools key --key 0.0.1 Enter` | Клавиша (Enter/Tab/Escape) |
| `light devtools shot --path shots` | Скриншот → PNG (без `--path` — base64 на stdout) |
| `light devtools session start` | Начать запись сессии |
| `light devtools session stop` | Завершить сессию, генерировать summary.md |

Все подкоманды принимают `--port` (по умолчанию 9229) и `--json` (машино-читаемый вывод).

## Автоматическая проверка компонентов

Перед `light dev` и `light build` CLI сканирует шаблоны проекта и проверяет,
все ли используемые компоненты установлены. Если найдены недостающие:

```
⚠️ Обнаружены недостающие компоненты:
   - light-fw-element-form (для <select>)
   - light-fw-behavior-validation (для required)
Установить их автоматически? (Y/n):
```

При ответе `Y` (или пустой строке) компоненты устанавливаются автоматически
(как `light add`). При `n` выводятся ручные инструкции и команда завершается
с кодом 1.

## Коды возврата

| Код | Значение |
|---|---|
| 0 | Успех |
| 1 | Ошибка (напр., отказ от установки компонентов) |
