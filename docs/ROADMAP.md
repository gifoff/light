# Roadmap Light

## Версия 0.2 – 0.10.3
*(Исторические этапы: создание CLI, компонентной модели, softbuffer, миграция на новую архитектуру).*

## Версия 0.14.0 (выполнено)

- Hot Reload: `light dev` — .rs rebuild + swap, HTML-перезагрузка без
  пересборки ✓.
- `#[default(expr)]` в `#[proxy]` ✓.
- DevTools TCP-сервер (127.0.0.1:9229): SNAPSHOT, CLICK, PING, QUIT ✓.
- `light.config.json` с профилями dev/prod ✓.
- Пример `examples/devtools-demo` ✓.

## Версия 0.15.0 (выполнено)

- `DeferredMinimize`, `WindowGeometry.minimized` ✓.
- Env vars для supervisor: `LIGHT_STATE_FILE`, `LIGHT_WINDOW_FILE`,
  `LIGHT_RESUME_FROM_SWAP`, `LIGHT_SWAP` ✓.
- `#[transient]` удалён → computed-методы `impl App` ✓.
- `light dev`: geometry при swap, user close удаляет state+window,
  `--preserve-state` ✓.
- Кириллица/пробелы, `parse_align_items("center")`, `eval_field` методы ✓.

## Версия 0.15.1 (выполнено)

- Backward-compat fix: компиляция примеров с дефолтными фичами ✓.
- Drift-фикс: `last_applied_geometry` вместо `outer_position()` ✓.
- Graceful `SAVE_ALL` перед kill при hot-swap ✓.
- CI: `build-default` джоб, `CARGO_BUILD_JOBS=1` на Windows ✓.

## Версия 0.15.2 – 0.15.5 (выполнено)

- **0.15.2**: Fix `light init` использует `run_with_config` и `dev-mode` feature ✓.
- **0.15.3**: Удален debug-спам `[save]` и `[moved]`, auto-detect пакета в `light dev` ✓.
- **0.15.4**: Auto-invalidate для `light dev` (отслеживание версии) ✓.
- **0.15.5**: Отключено принудительное растягивание Column по `width=100%` (фикс центрирования текста) ✓.

## Версия 0.16.0 (next) — Agentic DevTools

**Цель:** сделать Light удобным для автономной разработки с помощью ИИ-агентов (OpenCode, Cursor, Copilot Workspace). Агент должен уметь «увидеть» окно, «потрогать» UI и получить готовый стек действий для анализа.

### 1. CLI-инструменты для управления приложением (Headless)

Добавить в CLI команды для взаимодействия с запущенным приложением (через DevTools TCP-порт):

- `light devtools shot [--path <dir>]` — сделать скриншот активного окна (сохраняется в PNG).
- `light devtools click --key <key>` или `--coords <x,y>` — симулировать клик.
- `light devtools type --key <key> "text"` — ввести текст в поле.
- `light devtools state` — получить JSON-снимок реактивного состояния.
- `light devtools tree` — получить JSON-дерево VDOM с координатами и ключами.
- `light devtools find --tag <t> --text <s>` — найти элемент и вернуть его координаты.

### 2. Action Log & Session Recorder

При использовании любой команды `light devtools …` автоматически:

- Создаётся уникальная сессия в `.light/agent-sessions/<timestamp>-<rand>/`.
- В неё пишутся:
  - `actions.jsonl` — последовательность действий (команда, аргументы, время, результат).
  - `screenshots/` — снимки экрана до/после действия (если применимо).
  - `state/` — снимки состояния и VDOM на каждом шаге.
- В конце сессии генерируется `summary.md` с хронологией, ссылками на скриншоты и кратким отчётом (что изменилось).

### 3. Интеграция с `light dev`

- Команда `light dev --agent` запускает приложение в режиме, где все действия агента логируются в сессию.
- Возможность приложить сессию к отчёту (агент может прочитать `.light/agent-sessions/.../summary.md` и приложить скриншоты).

### 4. Документация

- В `AGENTS.md` добавить раздел «Как агенту тестировать UI» с примерами команд и ссылками на сессию.
- В `guide.md` — раздел для разработчиков «Автоматизированное тестирование с ИИ».
- В `README.md` — краткое упоминание «Light-friendly for AI Agents».

---

## Версия 0.17.0 (next+1) — Web UI DevTools

- **Web UI DevTools**: браузерный интерфейс для инспекции VDOM, State и логов.
- `light-devtools serve` (Axum + WS-прокси к 9229).
- Визуализация дерева компонентов и состояний в реальном времени.
- Возможность подключаться к сессии агента и просматривать Action Log в браузере.

---

## Версия 0.18.0+ — Ядро и расширяемость

- **Fix Layout**: Полная поддержка flexbox-модели для текстовых узлов и вложенных контейнеров (баг с `<div>` внутри `align-items="center"`).
- **Plugin API**: Возможность создавать новые теги (`NodeKind`) из сторонних крейтов без форка ядра.

---

## Не в плане

- Замена Rust на другой язык.
- Свой менеджер пакетов вместо Cargo.
- Свой реестр вместо crates.io.