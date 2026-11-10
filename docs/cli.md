# CLI Light

`light` — основной интерфейс работы с проектами Light: инициализация,
разработка, сборка, управление компонентами, публикация.

## Установка

```bash
cargo install light
```

## Команды

### `light init [ПАКЕТ] [--template NAME] [--with COMPONENT]`

Создаёт новый проект Light. Без аргументов — пустой проект. Флаг `--with`
подключает компоненты (форм, валидации).

```bash
light init my-app                    # пустой проект
light init my-app --template counter # шаблон counter
light init my-app --with form --with validation
```

Доступные showcase-шаблоны: `counter`, `todo`, `calculator`, `dashboard`.

### `light dev [ПАКЕТ] [--agent] [--verbose]`

Запускает приложение с горячей перезагрузкой (HTML + `.rs`) и сохранением
состояния через DevTools. Перед запуском автоматически проверяет наличие
недостающих компонентов.

Флаг `--agent` создаёт сессию для автономного тестирования UI через
`light devtools`.

```bash
light dev                # hot-reload
light dev --agent        # + сессия агента
```

### `light build [ПУТЬ] [-r] [-f FEATURES] [--no-default-features] [EXTRA...]`

Собирает приложение в release-режиме (`cargo build --release`). Перед сборкой
автоматически проверяет недостающие компоненты. После сборки выводит путь к
бинарному файлу и его размер.

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
light publish
light publish --yes --skip-gates
```

### `light edit КОМПОНЕНТ`

Открывает исходники компонента в редакторе (VS Code / Neovim / nano).

### `light search ПОДАЧА`

Поиск компонентов по описанию/функциям.

### `light vendor ПУТЬ`

Вендорит источники компонентов в локальный каталог (офлайн-режим).

### `light backup ПУТЬ`

Создаёт резервную копию проекта.

### `light devtools`

Инструменты для автономного тестирования UI (работает при запущенном
`light dev --agent`):

| Команда | Описание |
|---|---|
| `light devtools state` | JSON-состояние приложения |
| `light devtools tree` | Дерево элементов |
| `light devtools shot --path shots` | Скриншот → PNG |
| `light devtools find --tag button --text "+1"` | Поиск элемента |
| `light devtools click --key 0.0.0` | Клик |
| `light devtools type --key 0.0.1 "Hello"` | Ввод текста |
| `light devtools key --key 0.0.1 Enter` | Клавиша |
| `light devtools session stop` | Завершить сессию, генерировать summary.md |

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
