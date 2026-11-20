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

## Версия 0.16.0 (выполнено) — Agentic DevTools

**Цель:** сделать Light удобным для автономной разработки с помощью ИИ-агентов (OpenCode, Cursor, Copilot Workspace). Агент должен уметь «увидеть» окно, «потрогать» UI и получить готовый стек действий для анализа.

### 1. CLI-инструменты для управления приложением (Headless)

Команды `light devtools …` взаимодействуют с запущенным приложением через
DevTools TCP-порт (`127.0.0.1`, по умолчанию 9229; все принимают `--port` и
`--json`):

- `light devtools shot [--path <dir>]` — скриншот активного окна (PNG; при
  `--path` файл сохраняется в каталог, иначе выводится base64) ✓.
- `light devtools click --key <key>` — симулировать клик по ключу узла ✓.
- `light devtools type --key <key> "text"` — ввести текст в поле ✓.
- `light devtools key --key <key> Enter|Tab|Escape` — отправить именованную
  клавишу фокусному узлу ✓.
- `light devtools state` — JSON-снимок реактивного состояния (SNAPSHOT) ✓.
- `light devtools tree` — JSON-дерево VDOM (ключи, теги, текст, дети) ✓.
- `light devtools layout` — плоское дерево раскладки с rect x,y,w,h ✓.
- `light devtools find --tag <t> --text <s> --attr k=v` — найти элемент по
  тегу/тексту/атрибутам и вернуть его ключ + координаты ✓.
- `light devtools session start|stop` — управление session recorder ✓.

> Замечание: флаг `--coords` из черновика ТЗ намеренно **не реализован**
> (анти-фантом-фича); позиционирование идёт только по стабильным ключам узлов.

### 2. Action Log & Session Recorder

При использовании любой команды `light devtools …` автоматически:

- Создаётся уникальная сессия в `.light/agent-sessions/<timestamp>_<rand>/`
  (создаётся `session start`; при `light dev --agent` — автоматически) ✓.
- В неё пишутся:
  - `actions.jsonl` — последовательность действий (время, команда, аргументы,
    результат, ссылка на скриншот) ✓.
  - `screenshots/` — PNG-скриншоты (команда `shot` сохраняет их и
    автоматически дописывает ссылку) ✓.
  - `state/` — снимки состояния и VDOM на каждом шаге ✓.
- В конце (`session stop`) генерируется `summary.md` с датой, числом
  Actions/Screenshots и нумерованной хроникой команд ✓.

### 3. Интеграция с `light dev`

- Команда `light dev --agent` автоматически создаёт сессию до запуска,
  логирует все команды DevTools во время работы приложения и генерирует
  `summary.md` при выходе процесса ✓.
- Агент читает `.light/agent-sessions/<id>/summary.md` и прикладывает
  скриншоты из `screenshots/` к отчёту ✓.

### 4. Документация (выполнено)

- `AGENTS.md`: раздел «Testing UI with DevTools» — как агенту видеть/трогать
  UI и читать сессию ✓.
- `docs/guide.md`: раздел «Автоматизированное тестирование с ИИ» ✓.
- `README.md`: упоминание «Agent-friendly DevTools» ✓.

---

## Версия 0.18.0 (выполнено) — Web UI DevTools

- **Web UI DevTools**: браузерный интерфейс для инспекции VDOM, State и логов ✓.
- Крейт `light-fw-devtools-web` (tiny_http, без tokio/axum/tungstenite), HTTP 127.0.0.1:9228 + SSE (500 мс) ✓.
- Флаги `light dev --no-web` и `--web-port <ПОРТ>` ✓.
- Конфиг `dev.devtools.web.{enabled, port}` в `light.config.json`, приоритет флагов > конфиг > дефолт, fallback порта +1..+10 ✓.
- Визуализация дерева VDOM (ключи, теги, текст, дети) + панель State (SNAPSHOT) + Event Log ✓.
- Команды из браузера: Click selected, Screenshot (base64), Refresh ✓.
- JSON-контракт: HTTP → `{ok, result}`, SSE → `{kind, data}` ✓.

---

## Версия 0.19.0 (выполнено) — Plugin API v1

- **Plugin API**: `CustomElement` trait, `linkme`-реестр, `NodeKind::Custom`, runtime `on_event` dispatch ✓.
- Макрос `#[component(tag = "...")]` для регистрации кастомных тегов ✓.
- `light check`: 5 категорий (установлено, доступно, неизвестно, не используется, конфликты) ✓.
- `light add --force` + разрешение конфликтов в `light.lock` ✓.
- Auto-discovery кастомных тегов в `light dev` (TTY/non-TTY/`--no-auto-install`) ✓.
- Пример `examples/plugin-demo` (`<datepicker />`) ✓.
- Документация: `docs/cli.md`, `docs/guide.md`, `AGENTS.md` ✓.

---

## Версия 0.20.0+ — Ядро и расширяемость

- **Fix Layout**: Полная поддержка flexbox-модели для текстовых узлов и вложенных контейнеров (баг с `<div>` внутри `align-items="center"`).

## Версия 0.20.0 — Headless Mode & Modularity (Модульность и работа без GUI)

*Цель: разделить логику (VDOM, реактивность) и отрисовку (окно).*

- **Headless Mode**: Возможность сборки `light-fw-core` без `winit` и `softbuffer` (фича `headless`).
- **API для рендеринга**: Публичный метод для получения VDOM в виде строки (для SSR) или JSON (для тестов/агентов).
- **Вынос графики**: `softbuffer` / `wgpu` в отдельный крейт `light-fw-gui`. Ядро не знает о GUI.
- **Тестирование без окон**: Возможность запускать UI-компоненты в headless-режиме через `light test`.
- [x] Этап 1: скелет 5 новых крейтов (light-reactivity, light-vdom,
      light-gui, light-render-cpu, light-render-gpu) + workspace + зависимости.
- [ ] Этап 2: перенос реактивности (src/reactive/ → light-reactivity).
- [ ] Этап 3: перенос VDOM + layout (vdom.rs, layout.rs, dom.rs → light-vdom).
- [ ] Этап 4: перенос окна + a11y (window/, control.rs, a11y/ → light-gui).
- [ ] Этап 5: перенос CPU-рендера (render/softbuffer.rs, window/paint.rs → light-render-cpu).
- [ ] Этап 6: перенос GPU-рендера (render/wgpu.rs → light-render-gpu).
- [ ] Этап 7: `light-fw` становится фасадом, `light-fw-core` помечен deprecated.

---

## Версия 0.21.0 — UI Foundation (CSS и базовые виджеты)

*Цель: сделать фреймворк пригодным для верстки сложных интерфейсов.*

- **CSS-каскады**: Поддержка стилей через классы (`.class`), каскадирование, приоритеты.
- **Внешние стили**: Возможность подключения `.css` файлов.
- **Базовые виджеты** (через Plugin API): Tooltip, Modal/Alert, ProgressBar, Toast.
- **Иконки**: Компонент `<icon name="..." />`.

---

## Версия 0.22.0 — Data & Metrics (Таблицы и метрики)

- **Виджеты данных**: Table (виртуализированный скроллинг), List (динамические списки).
- **Performance Metrics**: Сбор FPS, времени клика, Diff-а, Layout-а. Вывод в Web UI DevTools.

## Версия 0.23.0+ — Ecosystem & Cross-platform
- **Кросс-компиляция**: CI-матрицы для сборки под Linux/Windows/macOS из любой ОС.
- **Каталог компонентов**: Развитие `light search` и `light check`.

---

## Не в плане

- Замена Rust на другой язык.
- Свой менеджер пакетов вместо Cargo.
- Свой реестр вместо crates.io.