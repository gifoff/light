# Changelog

Все заметные изменения проекта. Формат основан на [Keep a Changelog](https://keepachangelog.com/ru/1.1.0/), версии — по [Semantic Versioning](https://semver.org/lang/ru/).

## [0.17.1] - 2026-11-11

### Fixed

- **cli**: `light add` и `light init` больше не добавляют в `Cargo.toml`
  `default-features = false` для метакрейта `light-fw`: при добавлении фичи
  строковая запись `light-fw = "0.17"` превращается в
  `{ version = "0.17", features = [...] }` без отключения дефолтных фич
  (`<window>` и другие базовые элементы продолжают работать). Явно прописанный
  пользователем `default-features = false` сохраняется как есть. Добавлены
  регрессионные тесты (`edit.rs`, `init.rs`).
- **cli**: Исправлены устаревшие тестовые фикстуры в `edit.rs` — переименование
  метакрейта `light` → `light-fw` (в старых проектах ещё мог встречаться
  `light`).
- **cli**: В **неинтерактивном режиме** (CI, пайплайн, `echo Y | light build`)
  `light build` / `light dev` больше не зависают на блокирующем чтении stdin:
  недостающие компоненты устанавливаются автоматически с логом
  `light: auto-installing <component>`. В интерактивном режиме (stdin — TTY)
  по-прежнему спрашивает «(Y/n):».
- **components**: Обновлено требование к ядру в манифестах компонентов
  `light-fw-element-form` и `light-fw-behavior-validation` (`core = "^0.6"` →
  `^0.17`), что устраняло ложный отказ `light add` в workspace-разработке, где
  компонент разрешается из локального каталога `crates/`.

### Added

- **cli**: Флаг `--no-auto-install` для `light build` и `light dev`: показывает
  список недостающих компонентов и инструкции `light add`, но не устанавливает
  их автоматически.

## [0.16.3] - 2026-11-10

### Fixed

- **vdom**: Невалидное значение `align-items` (например, опечатка) больше не вызывает панику в debug-сборке. Вместо этого выводится warning в stderr, и используется значение по умолчанию `start`.
- **window**: Дефолтная иконка окна заменена на новый логотип (128x128 RGBA, синий шеврон). Зашита в бинарник через `include_bytes!` (без PNG-декодера).

### Changed

- **docs**: Финальная синхронизация документации по итогам релизов 0.16.0–0.16.3:
  раздел «Сборка и запуск» (`docs/quickstart.md`, сокращённый абзац в
  `README.md`, подраздел в `docs/guide.md`); упрощены пункт 2 (скриншот) и
  пункт 9 (центрирование заголовков) в `AGENTS.md`; в `docs/guide.md` —
  подраздел «Сохранение скриншота» (три формы `--path` + примеры редиректа),
  подраздел «Заголовки h1–h5 при align-items», уточнение о реальных датах
  в `summary.md`.

## [0.16.2] - 2026-11-09

### Fixed

- **`layout`: заголовки `<h1>`–`<h5>` при `align-items=center`** — ширина
  заголовка вычислялась как 0 (текст обрезался/«исчезал»). Теперь узлы `h*`
  (Column с тегами h1–h5) измеряются в бесконечном контент-боксе и получают
  max-content-ширину, после чего центрируются по cross-оси; дочерние
  Percent-тексты растягиваются до фактической ширины. Реализация — в
  контейнерной ветке раскладки (Button-arm model); добавлены 3 интеграционных
  теста (`heading_h1_gets_positive_width_under_align_items_center`,
  `headings_h1_to_h5_all_get_positive_width_when_centered`,
  `plain_div_with_centered_text_is_not_max_content_sized`).
- Документация синхронизирована с поведением 0.16.x DevTools: `AGENTS.md`
  (раздел «Testing UI with DevTools» — автоименование `shot --path`, сырой
  base64 в stdout, читаемые ошибки, новый пункт о центрировании заголовков),
  `README.md` (раздел «Agent-friendly DevTools»), `docs/guide.md`
  («Сохранение скриншота», реальные `Started`/`Ended` RFC3339 в `summary.md`,
  «Заголовки h1–h5 при align-items»).

## [0.16.0] - 2026-11-08

### Added — Agentic DevTools

- **`light devtools <cmd>`**: headless-инструменты для ИИ-агентов, общаются с
  запущенным приложением по DevTools TCP-порту (по умолчанию 9229; все
  принимают `--port` и `--json`):
  - `tree` — JSON-дерево VDOM (ключи, теги, текст, дети).
  - `layout` — плоская раскладка с координатами `x,y,w,h`.
  - `find --tag <t> --text <s> --attr k=v` — поиск элемента по тегу/тексту/
    атрибутам → `{key, tag, rect}`. Кнопка ищется по её подписи.
  - `state` — JSON-снимок реактивных полей (протокольная `SNAPSHOT`).
  - `click --key <key>` — симуляция клика по ключу узла.
  - `type --key <key> "text"` — ввод текста в поле.
  - `key --key <key> Enter|Tab|Escape` — именованная клавиша фокусному узлу.
  - `shot [--path <dir>]` — PNG-скриншот (base64 на stdout или в каталог).
  - `session start|stop` — управление session recorder.
- **Session Recorder**: `.light/agent-sessions/<id>/` с `actions.jsonl`
  (время, команда, аргументы, результат, ссылка на скриншот), каталогами
  `screenshots/` и `state/`, и генерацией `summary.md` при остановке.
- **`light dev --agent`**: автоматически создаёт сессию до spawn приложения,
  логирует команды DevTools во время работы и пишет `summary.md` при выходе.
- **Новые протокольные команды** TCP-сервера DevTools: `TREE`, `LAYOUT`,
  `FIND <json>`, `TYPE <key> <text>`, `KEY <key> <name>`, `SHOT`.
- Публичные хелперы `light-fw-core`: `VElement::collect_plain_text_public`
  (поиск кнопки по подписи), `serialize_tree`, `serialize_layout`,
  `find_element`, `encode_screenshot`.

### Notes

- Флаг `--coords` из черновика ТЗ намеренно **не** реализован (анти-фантом):
  позиционирование идёт только по стабильным ключам узлов.
- Документация обновлена: `AGENTS.md` (раздел «Testing UI with DevTools»),
  `docs/guide.md` (CLI-обёртка + «Автоматизированное тестирование с ИИ»),
  `README.md`, `docs/ROADMAP.md`.

## [0.15.5] - 2026-11-08

### Fixed

- **Layout**: Колонки (`Column`) больше не получают принудительный `width=100%` внутри `align-items="center"`. Текстовые узлы (и любые auto-sized Column) теперь корректно центрируются по горизонтали.
- **CLI**: Сравнение версий фреймворка происходит по `major.minor` (0.15.3 ≡ 0.15). Убран ложный warning при hot-build.
- **CLI**: Удалён устаревший debug println при запуске `light dev`.

## [0.15.4] - 2026-11-08

### Fixed

- **`light init`**: `src/main.rs` теперь использует
  `light::run_with_config("light.config.json", App::new())`
  вместо `light::run(App::new())`. DevTools запускаться
  корректно при `light dev` (профиль `dev` в `light.config.json`
  включает devtools на порту 9229).
- **`light dev` auto-invalidation**: версия фреймворка ищется
  не только в `[workspace.package].version`, но и в
  `[dependencies] light = "0.15"` (обёрточный крейт) или
  `[dependencies] light-fw-core = "0.15"`. Проекты, созданные
  `light init` (не workspace), теперь проходят auto-invalidation
  при смене версии.
- **Layout h1 centering**: заголовки `h1`–`h5` больше не
  растягиваются на 100% по ширине родителя (`align-items != Start`).
  Остаются авто-размера (max-content); текст внутри центрируется
  через `justify-content="center"`. `<div align-items="center">`
  + `<h1>` → заголовок по центру.

## [0.15.3] - 2026-11-08

### Fixed

- **`light dev` без positional**: auto-detect package из cwd
  (`Cargo.toml` вверх по дереву или `cargo metadata`).
  Больше не требует явного `light dev <name>`.
- **`light dev` feature path**: `--features light/dev-mode`
  (crate-qualified), работает без `[features] dev-mode` в
  Cargo.toml пользователя.
- **`light init`**: генерирует template-based skeleton
  (`templates/app.html`, `#[template]` + `#[proxy]`,
  `[features] dev-mode = ["light/dev-mode"]` в Cargo.toml).
- **Layout centering**: `align-items != Start` больше не
  stretch'ит child по cross-оси — align pass центрирует
  auto-sized child. `<div align-items="center">` центрирует
  потомков по горизонтали (Column) / вертикали (Row).
- **Debug eprintlns**: `[save]`, `[moved]`, `[apply]`,
  `runtime-templates DISABLED` — только под
  `#[cfg(feature = "devtools")]`. В `cargo run` без devtools —
  тишина.
- Удалён устаревший debug println "light dev: запускаю ...".

## [0.15.2] - 2026-11-08

### Fixed

- `light init` записывает в новый `Cargo.toml` диапазон версий
  `light = "0.15"` (minor), а не привязку к yanked-патчу `light = "0.15.0"`.

## [0.15.1] - 2026-11-07

### Fixed

- Регрессия 0.15.0: все примеры и приложения на `light = "0.15"` не
  компилировались с дефолтными фичами. Причин было несколько:
  - `Component::inspect_state` / `inject_state` были за `#[cfg(devtools)]`
    в трейте, но макрос `#[template]` генерировал реализацию без cfg →
    «method is not a member of trait». Методы теперь в трейте всегда.
  - `serde` и `serde_json` были опциональными зависимостями
    (`dep:serde`, `dep:serde_json`), привязанными к фичам `window` /
    `devtools` / `runtime-templates`. Сгенерированный код ссылается на
    `::light::serde` и `::light::serde_json` в любом наборе фич. Зависимости
    теперь неопциональные, реэкспорт без cfg.
  - `VElement::to_json` был за `#[cfg(devtools)]`; код макроса требует его
    всегда. Теперь доступен без фичи.
  - `#[proxy]`: док-комментарии на полях ломали сборку
    «unknown field attribute #doc» — теперь пропускаются.
  - Дискриминатор формы вызова обработчика в `event_branches`:
    `show_tab('overview')` (явные аргументы) и `Dom::…` (выражения)
    генерировались как метод-ссылка → E0618/E0615. Теперь полновесный вызов
    определяется по непустым аргументам или `::`.
  - `handler_call` не инжектнул аргумент события для безаргументных
    имён: `@mousedown="down"` давал `self.down;` (E0615). Теперь
    `self.down(*__button)` и т.п.
  - `eval_field`: v-for переменные (`task in tasks`) ошибочно считались
    методами → `self.task()` (E0599). Добавлен `collect_v_for_items` —
    имена циклов исключаются из веток оценки.
- `invoke_arms` (runtime `invoke_handler`): всегда `#call(); true` —
  простые имена методов, проверенные `is_plain_click_handler`.
- CI: новый джоб `build-default` собирает примеры без `--all-features`
  (ловит повторение регрессии). `CARGO_BUILD_JOBS=1` на Windows (D8050
  ring/reqwest).
- `scripts/check-features.ps1`: добавлена сборка примеров с дефолтными
  фичами; `$env:CARGO_BUILD_JOBS = "1"`.
- Drift позиции окна при minimize + hot-swap: `flush_geometry` читал
  `outer_position()` (WM-наблюдаемую, с frame-offset коррекцией), и следующий
  swap применял сдвинутую координату (+26 px за swap). Теперь сохраняется
  `last_applied_geometry` (desired), не observed. Drift = 0 за 5 свопов.
- `WindowState.last_applied_geometry` — источник истины для save; обновляется
  при `Moved`/`Resized` (вне restore) и при apply restore.
- TCP-command `SAVE_ALL`: перед kill старого процесса supervisor шлёт
  `SAVE_ALL\n` через 9229, ждёт `ok` до 500 ms — graceful save state+geometry
  без debounce. При таймауте — fallback TerminateProcess.

### Notes

- Ring D8050 (MSVC command-line overflow): `CARGO_BUILD_JOBS=1` рекомендован
  для Windows (см. AGENTS.md).
- 2 dashboard avatar-теста падают локально (`PermissionDenied` в
  `std::env::temp_dir()` под sandbox), проходят в CI.

## [0.15.0] - 2026-11-06

### Added

- `UserEvent::DeferredMinimize { window_id }` — публичный вариант enum для
  отложенной минимизации окна после первого render (решает пустой taskbar-
  thumbnail при spawn minimized). **Breaking для `match`:** `UserEvent` не
  `#[non_exhaustive]` — добавьте `_ => {}` в exhaustive matches.
- `WindowGeometry { minimized: bool }` — сохранение свёрнутости в
  `.light/window.json`, применение после первого кадра при swap.
- Env vars для унификации путей в supervisor:
  `LIGHT_STATE_FILE`, `LIGHT_WINDOW_FILE`, `LIGHT_RESUME_FROM_SWAP`,
  `LIGHT_SWAP`. Раньше пути собирались по-разному в dev и window crate.

### Changed

- `#[transient]` удалён; computed-значения теперь — методы `impl App`,
  пересчитываются при каждом render, state — поля struct, сохраняются в
  `.light/state.json`. `inspect_state` / `inject_state` макрос генерирует для
  всех полей, а не только transient.
- `light dev`: geometry применяется только при swap (`LIGHT_RESUME_FROM_SWAP=1`),
  при user close — state+window удаляются supervisor'ом; `pending_swap`
  защищает файлы от удаления при rebuild.
- `light dev`: `.rs` swap сбрасывает state по умолчанию; флаг
  `--preserve-state` сохраняет.
- Версии всех крейтов воркспейса и межкрейтовых зависимостей — `0.15.0`.

### Fixed

- Кнопка `Auto` ширины игнорирует `forced_w`, использует `max-content` с
  учётом padding (3 коммита: 34d4b83, a4b8128, afbb52c — уже на origin/master).
- Кириллица/пробелы отображались как □: `glyph_for` фильтрует basic chars
  через primary font (Segoe UI), emoji fallback — только для не-basic codepoints.
  Emoji шрифт: `seguiemj.ttf` (не `segoeuiemoji.ttf`).
- `parse_align_items` не поддерживал `center`.
- `eval_field` не генерировал ветки для методов — computed пересчитываются
  корректно.
- Supervisor не удалял `.light/` при user close (X) из-за смешения
  `pending_swap` / user close.
- Унифицирован `LIGHT_WINDOW_FILE` между `light-dev` и `light-window` (fix8):
  ранее geometry читалась по разным путям.
- `is_garbage()` фильтрует координаты `(-32000, -32000)` (свёрнутые окна).

### Notes

Web UI DevTools (`light-devtools serve`, Axum + WS-прокси к 9229) перенесён
в **0.16.0** — не входит в этот релиз.

## [0.14.0] - 2026-11-04

### Added

- `light dev`: Hot Reload для HTML-шаблонов (RELOAD_TEMPLATE без пересборки) и
  исходников `.rs` (пересборка + swap бинарника с сохранением состояния через
  `SAVE_STATE`). Наблюдатель на PollWatcher (500ms), debounce (HTML 100ms /
  .rs 500ms), hash-dedup notify-событий. Флаг `--verbose` для подробных логов.
- `#[default(expr)]` в макросе `#[proxy]`: значение по умолчанию для поля
  реактивного struct, задаётся выражением (не только `Default::default()`).
- DevTools TCP-сервер (127.0.0.1:9229): JSON-снимок UI (`SNAPSHOT`), управление
  кликами (`CLICK <key>`), `PING`, `QUIT`.
- `light.config.json` с профилями `dev`/`prod`; `run_with_config()`.
- Пример `examples/devtools-demo` с демонстрацией DevTools и TCP-клиента.

### Changed

- `light dev`: убраны verbose-логи (hash-dedup, debounce, notify events) из
  основного вывода; доступны через `--verbose`.
- Версии всех крейтов воркспейса подняты до `0.14.0`.

## [0.13.0] - 2026-11-01

### Added

- DevTools: TCP-сервер на 127.0.0.1:9229, к которому может подключиться
  IDE-плагин или внешний инспектор.
- JSON-снимок состояния: `DevToolsSnapshot` с полями `window_token`,
  `vdom`, `state`, `layout` (сериализация VDOM и реактивных свойств
  через `serde` + `serde_json`).
- Протокол управления: `SNAPSHOT`, `PING`, `CLICK <key>`, `QUIT`.
  Клик из TCP-клиента вызывает `dispatch` в UI-потоке через
  `EventLoopProxy` (wake-up), с ответом `ok:true/false`.
- Атрибут `key` в шаблонах — статическая адресация узла для
  DevTools-кликов. Экспортируется в JSON-снимке VDOM.
- `light.config.json` с профилями `dev`/`prod`.
- `run_with_config(path, app)`: читает JSON-конфиг, включает DevTools
  на указанном порту, если фича `devtools` активна.
- `.cargo/config.toml` с алиасами `cargo dev` / `cargo prod`.
  `light init` создаёт оба файла автоматически.
- Пример `examples/devtools-demo` с демонстрацией TCP-клиента.

### Changed

- `Component: Any` supertrait + метод `as_any_mut` для downcast-а
  при асинхронных обновлениях (из 0.13-A).
- `run_with_config` доступна только под `cfg(feature = "window")`;
  внутренний блок DevTools — под `cfg(feature = "devtools")`.

### Fixed

- `inspect_state` в макросе `#[template]` теперь сериализует примитивы
  через геттеры (`u32`, `String`, `bool`, `f64`), сложные типы — как
  `null` (без паники).
- DevTools в release-сборке не стартует (контракт `cargo run` без
  фич vs `cargo dev`).
- Защита от `unused import` при сборке без `a11y-winit`.

## [0.12.0] - 2026-10-31

### Added

- Управление фоновыми задачами: `TaskContext` и `TaskHandle::abort()`
  для кооперативной отмены. `spawn` теперь принимает замыкание с
  `TaskContext`.
- Мультиоконный таргетинг: `post_update_to` для адресного обновления
  конкретного окна по `WindowToken`. Добавлена функция
  `current_window_token()`.
- Поддержка `async`/`await`: фича `async-await` и функция
  `spawn_async`, работающая без привязки к внешним рантаймам
  (через `futures`). Добавлена зависимость `futures-timer` для
  асинхронных задержек.
- Пример `examples/multi-window`, демонстрирующий независимые
  фоновые задачи в разных окнах.

### Fixed

- Исправлено зависание UI при использовании асинхронных задержек
  (убран busy-wait).
- Исправлена выдача `current_window_token()` для корневого окна.
- Починены тесты `light-cli`, которые падали из-за хардкода
  версий при обновлении мажорной версии.

## [0.11.0] - 2026-10-27

### Added

- Асинхронный рантайм: приложение теперь работает с полноценным асинхронным
  рантаймом. Обработчики событий и таймеры запускаются как async-задачи и не
  блокируют отрисовку; длительные фоновые задачи (сеть, I/O) выполняются
  параллельно с UI.
- `Task` — тип для запуска и отмены фоновых задач. Задачи живут внутри
  приложения, корректно отменяются при завершении работы и не теряют
  сообщения при переотрисовке.
- Таймеры (`spawn_later` / периодические) переписаны на асинхронный рантайм:
  они не блокируют цикл отрисовки и не накапливают задержку при длительных
  обработчиках.
- Публичный API для отправки сообщений и управления задачами из обработчиков
  и из самих асинхронных задач.

### Fixed

- Устранена потеря сообщений при переотрисовке: асинхронная задача, запущенная
  в обработчике, больше не «отваливается» после того, как элемент перерисован
  и обработчик заменён на новый.
- Таймеры не накапливают задержку: периодические таймеры считаются от момента
  срабатывания, а не от запланированного момента, поэтому длительные
  обработчики больше не сдвигают ритм.
- Устранено блокирование отрисовки в обработчиках: длительная работа в
  обработчике события больше не останавливает цикл отрисовки на всё время
  выполнения.
- `light publish` теперь корректно упаковывает воркспейс: при сборке dry-run
  все 8 крейтов пакуются, а их версии совпадают с версией воркспейса.

## [0.10.3] - 2026-10-23

### Fixed

- Устранён «фантомный» скроллбар: при переключении на вкладку, где контент
  помещается без прокрутки, полоса прокрутки больше не остаётся на экране.
  Область старой полосы теперь всегда попадает в damage-регионы, а отрисовка
  закрашивает её фоном контейнера (строгая проверка `scroll_height > client_height`).
- Сброс прокрутки: когда контент перестаёт прокручиваться, сохранённое
  `scroll_offset` удаляется, поэтому после возврата большого контента он не
  открывается наполовину прокрученным в пустоту.

## [0.10.2] - 2026-10-23

### Added

- Скроллбар теперь интерактивен: ползунок можно перетаскивать мышью, а клик по
  дорожке перелистывает содержимое на страницу (вверх или вниз в зависимости от
  положения клика относительно ползунка).

### Fixed

- `examples/dashboard`: левая панель зафиксирована по ширине (`240px`,
  `flex-shrink="0"`) и больше не сужается/расширяется от содержимого вкладок.
  Имя профиля и подпись «Product design» получили `white-space="nowrap"` и
  обрезаются с многоточием.
- `examples/dashboard`: имя профиля валидируется при сохранении (от 3 до 20
  символов). Короткое или длинное имя блокирует кнопку «Сохранить» и показывает
  красную подсказку под полем.
- `examples/dashboard`: аватар ограничен 100 КБ и размером 512×512. Слишком
  большой файл или превышение разрешения показывает ошибку под аватаркой, без
  замены текущего изображения.

## [0.10.1] - 2026-10-23

### Fixed

- Устранён регресс раскладки: scroll-контейнер больше не перекрывает `<titlebar>`
  при нехватке места — titlebar получил нулевой `flex-shrink`.
- Устранён лаг при переключении вкладок в `examples/dashboard`:

  - перерисовка при наведении ограничена областью старой и новой кнопки под
    курсором вместо всего окна;
  - пересекающиеся повреждённые области объединяются до отрисовки, чтобы не
    рисовать одно и то же дерево несколько раз за кадр;
  - непрозрачные заливки пишутся как строки пикселей без побуквенного
    alpha-смешивания;
  - измерения текста кэшируются (в рамках раскладки, а также посимвольные
    advance/высоты строк у шрифта), чтобы ускорить повторный layout.

## [0.10.0] - 2026-10-23

### Added

- Перенос текста по словам по умолчанию для текстовых узлов; атрибут
  `white-space="nowrap"` для однострочного текста с обрезкой через `…`.
- `flex-shrink` по умолчанию `1` (как в CSS) и минимальный размер `0` для
  flex-элементов. Дети scroll-контейнеров не сжимаются, чтобы не ломать
  прокрутку.
- Runtime-изображения: `ImageData` загружает и проверяет PNG во время
  выполнения, а `<image :src="...">` с резервным статическим `src="..."`.
- Cargo-фича `file-dialog` и обёртка `light::file_dialog::pick_image_file()`
  поверх `rfd` для нативного диалога выбора файла.

### Changed

- `examples/dashboard`: имя профиля стало текстом с карандашом `✎`; модалка
  профиля получила крупный аватар, выбор PNG по нажатию на аватар (до 2 МБ,
  не больше 1024×1024). Имя переносится по словам и не вылезает за карточку.
- Карточка «Открытые задачи» открывает список всех задач с кнопкой «Назад».

## [0.9.1] - 2026-10-22

### Fixed

- `examples/dashboard`: основная область ограничена высотой окна, поэтому
  вертикальная полоса прокрутки видна, а содержимое действительно
  прокручивается. Навигация sidebar прокручивается отдельно, профиль закреплён
  внизу и не уходит за экран.
- Профиль dashboard: имя показывается текстом с кнопкой-карандашом; изменение
  имени и аватарки перенесено в модальное окно.

### Changed

- Версии всех крейтов воркспейса подняты до `0.9.1`.

## [0.9.0] - 2026-10-22

### Added

- `overflow`/`overflow-x`/`overflow-y` (`visible`/`hidden`/`auto`/`scroll`):
  обрезка содержимого, вычисление `scroll_width`/`scroll_height`, прокрутка
  колесом мыши/трекпадом, полосы прокрутки и учёт обрезки в hit-test.
- `position="absolute"`: элемент выводится из потока, а координаты и
  `position-x`/`position-y` считаются от content box родителя. Подходит для
  модальных оверлеев и всплывающих панелей.
- Кэш растрированных глифов: повторная отрисовка текста (смена вкладок,
  hover) переиспользует coverage-битмапы вместо повторного outline-рендера.

### Fixed

- Растяжение по cross-оси: дети `Row`/`<titlebar>` с `height="auto"` растягиваются
  по высоте самого высокого соседа даже при отрицательном свободном месте.
- Дочерние окна (модалки) центрируются относительно родительского окна.

### Changed

- Пример `examples/dashboard`: модалка создания задачи стала встроенным
  оверлеем, добавление задачи обновляет счётчик, список событий и график;
  профиль позволяет редактировать имя и переключать аватар; контент
  прокручивается.
- `Style` дополнен полями `position`, `overflow_x`, `overflow_y`;
  `PlacedNode` — метаданными прокрутки и `clip_rect`.
- Версии всех крейтов воркспейса подняты до `0.9.0`.

## [0.8.0] - 2026-10-22

### Added

- Фича `image-support`: элемент `<image src="..." alt="...">` для PNG.
  Изображение читается на этапе компиляции и встраивается в бинарник через
  `include_bytes!`, размеры берутся из IHDR. При задании одной стороны вторая
  вычисляется с сохранением пропорций; `alt` попадает в дерево доступности.
  Декодированные пиксели кэшируются по `src`, отрисовка — nearest-neighbor с
  учётом альфы.
- Атрибут `icon` у `<window>`: PNG-иконка окна, встроенная в бинарник. Без
  атрибута используется встроенная иконка Light; при ошибке декодирования
  также происходит возврат к ней.
- `Component::open_modal()` — открытие модального окна из обработчика шаблона.
- Пример `examples/dashboard`: dashboard с изображениями, вкладками, прогрессом
  проектов и модальным окном создания задачи.
- Опциональная зависимость `png` (0.17) для декодирования изображений.

### Fixed

- Раскладка текста: `measure` учитывает только `ascent - descent`, а базовая
  линия в `draw_text` ставится на `y + ascent`; текст больше не смещается вниз
  из-за line-gap шрифта. Исправлено центрирование и выравнивание по нижнему
  краю в кнопках, заголовках и текстовых полях.

### Changed

- `NodeKind` дополнен вариантом `Image` (под фичей `image-support`); сравнение
  узлов при diff учитывает `src`, `data` и `alt`.
- Версии всех крейтов воркспейса подняты до `0.8.0`.

## [0.7.0] - 2026-10-21

### Added

- `flex-grow`, `flex-shrink` и сокращение `flex` в шаблонах и
  `Style::flex_grow`/`Style::flex_shrink`: свободное место и переполнение
  распределяются по main-оси `Column`/`Row` с учётом весов.
- `Component::close()`, `Component::minimize()` и
  `Component::toggle_maximize()` — обработчики для кнопок кастомного titlebar:
  `@click="close()"` и т. п.
- Бенчмарки Criterion для VDOM diff, раскладки, прямоугольников и текста;
  [docs/BENCHMARKS.md](docs/BENCHMARKS.md) с методикой и результатами.
- Раздел Showcase в README со скриншотами примеров.

### Changed

- Примеры counter, todo и calculator получили кнопки сворачивания/закрытия,
  вертикальное центрирование заголовка и `resizable="false"`.
- Кнопочная сетка калькулятора заполняет свободную высоту окна, а строка ввода
  todo растягивается на доступную ширину без переполнения.
- Публичные крейты включают `#![deny(missing_docs)]`; документированы
  недостающие элементы, исправлены broken intra-doc links, `cargo doc` проходит
  без предупреждений.
- Встроенный реестр CLI и примеры манифестов обновлены до `0.7.0`/`^0.7`.

## [0.6.1] - 2026-10-21

### Fixed

- `light publish` больше не падает, если `cargo publish` сообщает, что версия
  уже есть в git-индексе crates.io (`already exists on crates.io index`,
  `already uploaded`). Такой крейт помечается как пропущенный, публикация
  остальных продолжается, а итоговый счётчик учитывает его как завершённый.

## [0.6.0] - 2026-10-20

### Added

- `light init --template counter|todo|calculator`: готовые приложения
  встраиваются в CLI через `include_str!` и копируются в новый проект вместе
  с HTML-шаблонами.
- Каталог `examples/` с showcase-приложениями counter, todo и calculator.

### Fixed

- Парсер `light-fw-html` больше не паникует на многобайтовых символах в начале
  и конце текстовых узлов.

## [0.5.0] - 2026-10-20

### Added

- `light add` поддерживает сторонние компоненты `light-fw-*`, отсутствующие во
  встроенном реестре: версия разрешается через crates.io, а `--features`
  передаёт фичи самого крейта. Флаг `--with` у `light init` подключает
  компоненты и нужные фичи мета-крейта.
- `light search <запрос>` ищет компоненты на crates.io и выводит таблицу
  «имя / версия / описание / ссылка»; показываются только `light-fw-*` и
  крейты с упоминанием Light в описании.
- `light publish` распознаёт rate limit crates.io (HTTP 429) и завершается с
  понятным сообщением без бесполезных ретраев.
- [docs/EXTERNAL_COMPONENTS.md](docs/EXTERNAL_COMPONENTS.md) — гайд по
  созданию и публикации сторонних компонентов.

### Changed

- Фреймворк переименован в Light. Мета-крейт называется `light`, компоненты
  имеют префикс `light-fw-`.

### Fixed

- `light publish` пропускает уже опубликованные крейты с логом
  `skipped (already published)` и продолжает публикацию остальных, вместо
  завершения с фатальной ошибкой.

## [0.2.0] - 2026-10-17

### Added

- Доступность (a11y) на базе AccessKit: сопоставление ролей, `aria-*`-атрибуты и построение дерева доступности (`light::a11y`).
- Интеграция с `accesskit_winit`: адаптер окна, отправка `TreeUpdate` при перерисовке и обработка `ActionRequest` от диктора.
- Управление фокусом и клавиатурная навигация: `Tab`/`Shift+Tab`, активация `Enter`/`Space`.
- Видимое кольцо фокуса у сфокусированного узла (`#4A90D9`, 2px) и программное управление фокусом: `focus(key)`, `blur()`, `focused_key()`.
- Примеры `a11y_form` и `a11y_dialog` (`cargo run --example …`).
- Фича `a11y-winit` (включена по умолчанию вместе с `window` и `a11y`; включает подсистемы `a11y-live` и `a11y-keyboard`).
- Проверка статического `tabindex` на этапе компиляции: нечисловое значение — ошибка сборки (динамический `:tabindex` проверяется во время выполнения).
- Курсоры мыши: автоматический выбор по роли узла (ссылки и элементы управления — `pointer`, поля ввода — `text`, отключённые — `not-allowed`, остальные — `default`), переопределение атрибутом `cursor`/`:cursor` и всплытие к ближайшему подходящему родителю (`light::vdom::Cursor`, `light::window::resolve_cursor`).
- Проверка статического `cursor` на этапе компиляции: неизвестное имя — ошибка сборки (динамический `:cursor` проверяется во время выполнения).
- Режим `--dump-a11y` в примерах `a11y_form` и `a11y_dialog`: печатает дерево доступности в JSON.
- Атрибут `z` (и динамический `:z`) задаёт порядок отрисовки и попадания среди соседей: больший `z` рисуется поверх и первым перехватывает события. При равных `z` сохраняется порядок в разметке. `z` не влияет на раскладку и действует локально для родителя.
- Построчные отступы `padding-top`/`padding-right`/`padding-bottom`/`padding-left` и `margin-top`/`margin-right`/`margin-bottom`/`margin-left`: переопределяют соответствующую сторону базовых `padding`/`margin`. Тип отступов — `light::vdom::Edges` (`Style::padding`/`Style::margin`), а `Style::inset()` теперь возвращает отступы по сторонам.
- Выравнивание **содержимого** контейнера: `justify-content` (main-ось) и `align-items` (cross-ось) со значениями `start`/`center`/`end`. `Column`/`<window>` — main по вертикали, `Row`/`<titlebar>` — по горизонтали. `position-x`/`position-y` конкретного ребёнка переопределяет `align-items` родителя (поля `Style::justify_content`, `Style::align_items`).
- Динамические привязки `:attr` для **всех** атрибутов: значения стилей (`:width`, `:height`, `:min-*`/`:max-*`, `:position-x`/`:position-y`, `:position-offset-*`, `:justify-content`, `:align-items`, `:padding`, `:margin`, `:border-size`, `:font-size`, `:background`, `:font-color`, `:border-color`, `:required`, `:required-indicator`) и типов узлов (`:type` у `<button>`, `:placeholder`/`:textarea`/`:resize` у полей, `:value` у `<radio>` и `<option>`) разбираются во время выполнения по тем же правилам, что и статические, и имеют приоритет над одноимённым статическим атрибутом. Новые публичные парсеры: `light::vdom::parse_size_or_auto`, `parse_offset`, `parse_align_x`/`parse_align_y`, `parse_justify`/`parse_align_items`, `parse_px`, `parse_color`, `parse_bool`, `parse_required_indicator`, `parse_button_type`, `parse_resize`.
- `diff` учитывает изменение атрибутов узла (`VElement::attrs`) и полей `NodeKind` без отдельной операции (`Radio::value`, `TextField::multiline`/`resize`): такой узел помечается изменившимся и перерисовывается.
- Гранулярные Cargo-фичи: код неиспользуемых элементов и подсистем вырезается из бинарника. Набор по умолчанию — `elements-basic`, `window`, `a11y-winit`; остальные возможности (элементы форм, `align-content`, `z-order`, `cursor-theming`, `offset-position`, `dynamic-attrs`, `required-fields`, `validation`, `min-max-size`, `window-*`, `a11y-*`, `include`) включаются явно. Сетка фич описана в [docs/DESIGN_PRINCIPLES.md](docs/DESIGN_PRINCIPLES.md) и [docs/guide.md](docs/guide.md).
- Понятная ошибка компиляции при использовании выключенного элемента: `<password>`/`<checkbox>`/`<radio>`/`<select>` без соответствующей фичи больше не превращаются в «неизвестный элемент» — макрос `#[template]` называет нужную фичу и подсказывает, как её включить.
- CLI `light` (`crates/light-cli`, ядро — `crates/light-cli-core`) с командами `check` и `suggest`: статически сканирует `*.html`-шаблоны проекта, сопоставляет использованные теги/атрибуты/директивы со встроенным реестром компонентов и показывает отсутствующие и неиспользуемые компоненты. Вывод в формате `text`/`json`, флаг `--strict` для строгого кода возврата; только чтение, без запуска Cargo. Компилятор шаблонов вынесен в отдельный крейт `light-fw-html`.
- CLI: `light doctor`, `light add`, `light remove`, `light update`, `light lock`, `light vendor`.
- `light.lock` for reproducible builds.
- Backups and automatic rollback for all CLI mutations.
- Workspace-component detection: `light add` uses local path for workspace members.
- Команда `light doctor [путь]` (`crates/light-cli`, ядро — `light-fw-cli-core`): проверяет версию `cargo`, доступность crates.io (при включённой фиче `network`), установленные компоненты и их совместимость с версией ядра. Вывод в формате `text`/`json`, флаг `--offline`; код возврата `1` при ошибках.
- Команда `light add <компонент> [путь]`: добавляет зависимость компонента в `Cargo.toml`, включает соответствующую фичу (имя компонента без префикса `light-`, переопределяется `--feature`), обновляет `light.lock` и проверяет сборку через `cargo check`. Команда `light remove <компонент> [путь]` удаляет зависимость и вычищает связанную фичу. Список устанавливаемых зависимостей и фич выводится в виде плана.
- Команда `light update [компонент] [путь]`: обновляет установленные компоненты до актуальных версий (для локальных — версия определяется крейтом, для crates.io — встроенным реестром), применяя изменения через `toml_edit` и фиксируя их в `light.lock`.
- Команда `light lock [путь]`: создаёт или обновляет `light.lock` (точные версии и источник компонентов, версия ядра), печатает разницу с текущим состоянием и поддерживает `--dry-run`.
- Команда `light vendor [путь]`: копирует исходники локальных компонентов в `vendor/` (исключая `target/` и `.git/`), чтобы проект собирался без сети и без соседних репозиториев. Компоненты, доступные только на crates.io, помечаются как пропущенные.
- Общие для изменяющих команд возможности: `--dry-run` (показать план), `--yes` (без вопросов), `--offline` (только workspace/vendor, без сети), `--no-verify` (не запускать `cargo check`). Источник компонента выбирается в порядке workspace → vendor → crates.io.
- Резервное копирование и откат: перед изменением `Cargo.toml`/`light.lock` снимок сохраняется в `.light/backup/`, а при неудачном `cargo check` файлы восстанавливаются (включая удаление появившихся файлов).
- Ядро `light-fw-cli-core` расширено модулями: `process` (запуск внешних команд и разбор версии), `cargo` (обёртки `cargo add`/`remove`/`check`), `manifest`/`edit` (чтение и правка `Cargo.toml` через `toml_edit`), `doctor`, `fetch` (разрешение источника компонента), `backup`, `lockfile` (создание/диф `light.lock`), `add`/`remove`/`update`/`vendor`. Сетевой доступ (`reqwest`) подключается фичей `network` (у `light-fw-cli` включена).
- Тесты `light-fw-cli-core` (модульные, с `MockRunner`) и интеграционные тесты `light-fw-cli` на временных копиях фикстур (`tests/fixtures/`).
- Атрибут `required` для `<text>`/`<password>`/`<textarea>`/`<checkbox>`/`<select>` и групп `<radio>`; настраиваемый индикатор `required-indicator` (`*`, `none` или цвет).
- Пользовательские проверки: `:validator="метод"` у поля (`fn(&self, value: &str) -> Result<(), String>`) и `@validate="метод"` у формы (`fn(&self) -> Result<(), Vec<(String, String)>>`).
- Атрибут `type` у `<button>`: `submit` (по умолчанию), `button`, `reset`.
- Модуль `light::validation`: `validate_required`, `validate_form`, `find_submit_button`, `ValidationResult`, `FieldError`, `form_ancestor`.
- Показ ошибок: красная рамка и `aria-description` у невалидных полей, фокус на первом невалидном поле, озвучивание сообщения диктором; ошибка снимается при изменении значения.
- Отправка формы клавишей `Enter` из однострочного поля (`<text>`/`<password>`) внутри `role="form"`: срабатывает первая включённая кнопка `type="submit"`.
- Новые поля `Style`: `required`, `invalid`, `required_indicator`; новое поле `NodeKind::Button`: `button_type`. Стили учитываются в `diff`, поэтому смена валидности перерисовывает узел.
- Тесты `tests/validation.rs`, `tests/events.rs`, `tests/form_submit.rs` и проверки доступности `required`/`invalid`/`aria-description` в `tests/a11y_tree.rs`.

### Fixed

- Заданный `padding` у `<button>` переопределяет встроенные отступы кнопки (14/9), а не добавляется к ним: `padding="0"` больше не оставляет скрытых внутренних отступов.
- Центрирование содержимого `<button>` больше не сдвигает детей по вертикали на горизонтальный отступ: контентная область кнопки учитывает встроенные отступы по каждой оси отдельно (9px по вертикали, а не 14px).
- Литерал `Style` в макросе `#[template]` больше не ломает сборку при добавлении новых полей в `Style`: недостающие поля берутся из `Default::default()`.
- Проценты (`width="100%"` и т. п.) у детей контейнера без явного размера больше не схлопываются в нулевую ширину: контейнер растягивается по `cross`-оси потока и служит базой для процентов. Из-за схлопывания в `0` кнопка с `width="%"` имела нулевую площадь и не нажималась.
- Проценты в `width`/`height`/`min-*`/`max-*` пересчитываются при изменении размера окна: раскладка строится заново против нового размера viewport, пересчёт идёт каскадно по всему поддереву.
- `TreeUpdate.focus` теперь указывает на узел с наименьшим положительным `tabindex`, если явный фокус не задан (иначе — первый tabbable в DOM-порядке, а при их отсутствии — корень).
- `placeholder` текстового поля используется как доступное имя (`label`), если не задан `aria-label`/`aria-labelledby`.
- Контейнеры больше не получают `label`, собранный из текста потомков: имя выводится из содержимого только для семантических ролей (`Button`, `Label`, `Link`, `Heading`, `CheckBox`, `RadioButton`, `MenuItem`).
- Отключённые узлы (`disabled`/`aria-disabled`) не получают события: клик по ним не вызывает `@click` и не всплывает к родителям, а курсор над ними — `not-allowed`.
- Клавиатурная активация (`Enter`/`Space`) отключённого узла игнорируется; узел, ставший отключённым в фокусе, уступает фокус следующему tabbable-элементу.
- Compile-time gates for opt-in elements now show helpful error messages.
- `light doctor` correctly detects `light-fw-core` version in workspace projects.
- `light add` is idempotent for already-installed components (exit 0).
- `light remove` updates `light.lock` and does not drop unrelated empty features.
- `light vendor --path` flag now parses correctly.

### Changed

- Монолит разбит на ядро и компоненты: код переехал в новый крейт `light-fw-core` (vdom, реактивность, раскладка, paint, окно, доступность), а крейт `light` стал тонкой мета-обёрткой, которая реэкспортирует ядро, макросы и подключённые компоненты. Добавлены компоненты `light-fw-element-form` (элементы `<checkbox>`/`<radio>`/`<select>`/`<option>`) и `light-fw-behavior-validation` (`required`, `:validator`, `@validate`); фичи `light/elements-form` и `light/validation` тянут соответствующие компоненты. Текстовые поля (`<text>`, `<password>`, `<textarea>`) входят в ядро. Публичный API сохранён: `use light::prelude::*;` и `#[template]`/`#[proxy]` работают без изменений.
- Встроенный реестр CLI приведён к новой структуре крейтов: удалён `light-element-text` (текстовые поля — это ядро), версии компонентов — `0.2`, требование к ядру — `^0.2`.
- `<button>` стал контейнером: подпись собирается из содержимого детей (голый текст и вложенные элементы), выкладывается по горизонтали и центрируется внутри кнопки по обеим осям. Поле `NodeKind::Button::label` удалено; обновление подписи идёт через обычный дифф детей (`Op::SetContent`). Вертикальная компоновка содержимого задаётся вложенным `<div direction="vertical">`.
- `position-x`/`position-y` принимают значение `center` (выравнивание по центру контентной области родителя).
- Позиционирование и размеры-проценты отсчитываются от **контентной области родителя** (границы минус `padding` и рамка), а не от окна: `position-x`/`position-y`, `position-offset-x`/`position-offset-y` и проценты в `width`/`height`/`min-*`/`max-*`. База берётся от итогового размера родителя. В вертикальном потоке (`direction="vertical"`, `<window>`) контейнер без явного `width` растягивается на доступную ширину, в горизонтальном (`direction="horizontal"`, `<titlebar>`) — без явного `height` по высоте; растяжение отключается явным размером по этой оси. Где растяжение не применяется (например, ширина контейнера в горизонтальном потоке), `Auto`-база равна `0`, и проценты по этой оси дают `0`. Корень и `<window>` занимают всё окно.
- Текст внутри заблокированной (`:disabled`) кнопки рисуется приглушённым цветом.
- Split `light` into `light-fw-core`, `light-fw-element-form`, `light-fw-behavior-validation`.
  Existing code using `light` continues to work with default features.
- Gated elements (`<select>`, `<checkbox>`, `<radio>`) now produce a clear
  compile-time error when used without the corresponding feature.

### Notes

- Поддержку доступности можно выключить переменной окружения `LIGHT_A11Y=0` или программно — `light::a11y::set_enabled(false)`.

## [0.3.0] - 2026-10-20

### Added

- `docs/COMPONENT_AUTHORING.md` — практическое руководство по созданию
  и публикации компонентов Light.
- Корневой `README.md` с кратким описанием, быстрым стартом и списком крейтов.
- Лицензии `LICENSE-MIT` и `LICENSE-APACHE` (двойное лицензирование MIT/Apache-2.0);
  копии добавлены в каталоги публикуемых крейтов, чтобы попадать в `.crate`.
- Метаданные crates.io для публикуемых крейтов: `description`, `license`,
  `repository` и `readme`.
- Команда `light publish [крейт]`: публикует крейты воркспейса в crates.io в
  топологическом порядке (граф строится из `cargo metadata`), пропуская
  `publish = false`. Pre-flight гейты (`cargo build`/`test`/`clippy`/`fmt`,
  строгая проверка шаблонов, валидация `[package.metadata.light]`, сверка версий
  внутренних зависимостей и проверка «уже опубликовано» через crates.io API и
  git-теги), ретраи на устаревание индекса crates.io с экспоненциальной задержкой,
  интерактивное подтверждение и флаги `--dry-run` (упаковка через
  `cargo package --list` без сети и без публикации), `--yes`, `--allow-dirty`,
  `--token`, `--verify`/`--no-verify`, `--skip-gates`, `--only`.
- Команда `light init <имя>`: создаёт скелет нового проекта Light — `Cargo.toml`
  с зависимостью `light`, самодостаточный `src/main.rs` (окно с «Hello, Light!»),
  `.gitignore` и `light.lock`. Каталог задаётся позиционно (`./<имя>`) или флагом
  `--path` (имя крейта при этом остаётся позиционным). Флаг `--minimal` создаёт
  только `Cargo.toml` и `src/main.rs`. Ошибки, если каталог не пуст или `cargo`
  не найден в `PATH`.

### Fixed

- `light add`/`light remove` сохраняют форматирование `Cargo.toml`: соседние
  `dep:`-элементы многострочных фич больше не склеиваются на одной строке, а
  комментарии не дублируются и не теряются. `add` + `remove` дают
  byte-perfect roundtrip.

## [0.4.0] - 2026-10-20

### Added

- Абстракция бэкенда рендеринга `RendererBackend`: программная растеризация
  отделена от показа кадра. `pixels`/`wgpu`/`naga` вынесены в `WgpuBackend`
  под фичей `wgpu-backend`; без графического бэкенда используется
  бэкенд-заглушка, а `cargo check --no-default-features` проходит без
  `wgpu`/`naga`.
- Бэкенд `SoftbufferBackend` под фичей `softbuffer-backend`: готовый
  RGBA-кадр показывается через `softbuffer` (формат `0x00RRGGBB`), без
  `wgpu`/`naga`. Фича `softbuffer-backend` поддерживает `light` и
  `light-fw-core`, а демо получило отдельные `softbuffer`/`wgpu`/`full-wgpu`
  для сборки обоих вариантов.

### Changed

- Набор по умолчанию (`light`, `light-fw-core`, `light-demo`) использует
  `softbuffer-backend` вместо `wgpu-backend`. Release-сборка демо уменьшилась
  с 7.86 MB до 1.70 MB (−6.16 MB, −78%). Если включены обе фичи, окно
  выбирает `softbuffer`.

