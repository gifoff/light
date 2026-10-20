# Changelog

Все заметные изменения проекта. Формат основан на [Keep a Changelog](https://keepachangelog.com/ru/1.1.0/), версии — по [Semantic Versioning](https://semver.org/lang/ru/).

## [Unreleased]

### Changed

- Пакет `relay-cli` переименован в `relay-framework-cli` из-за коллизии имени
  на crates.io. Исполняемый файл по-прежнему называется `relay`.

### Fixed

- `relay publish` пропускает уже опубликованные крейты с логом
  `skipped (already published)` и продолжает публикацию остальных, вместо
  завершения с фатальной ошибкой.

## [0.2.0] - 2026-10-17

### Added

- Доступность (a11y) на базе AccessKit: сопоставление ролей, `aria-*`-атрибуты и построение дерева доступности (`relay::a11y`).
- Интеграция с `accesskit_winit`: адаптер окна, отправка `TreeUpdate` при перерисовке и обработка `ActionRequest` от диктора.
- Управление фокусом и клавиатурная навигация: `Tab`/`Shift+Tab`, активация `Enter`/`Space`.
- Видимое кольцо фокуса у сфокусированного узла (`#4A90D9`, 2px) и программное управление фокусом: `focus(key)`, `blur()`, `focused_key()`.
- Примеры `a11y_form` и `a11y_dialog` (`cargo run --example …`).
- Фича `a11y-winit` (включена по умолчанию вместе с `window` и `a11y`; включает подсистемы `a11y-live` и `a11y-keyboard`).
- Проверка статического `tabindex` на этапе компиляции: нечисловое значение — ошибка сборки (динамический `:tabindex` проверяется во время выполнения).
- Курсоры мыши: автоматический выбор по роли узла (ссылки и элементы управления — `pointer`, поля ввода — `text`, отключённые — `not-allowed`, остальные — `default`), переопределение атрибутом `cursor`/`:cursor` и всплытие к ближайшему подходящему родителю (`relay::vdom::Cursor`, `relay::window::resolve_cursor`).
- Проверка статического `cursor` на этапе компиляции: неизвестное имя — ошибка сборки (динамический `:cursor` проверяется во время выполнения).
- Режим `--dump-a11y` в примерах `a11y_form` и `a11y_dialog`: печатает дерево доступности в JSON.
- Атрибут `z` (и динамический `:z`) задаёт порядок отрисовки и попадания среди соседей: больший `z` рисуется поверх и первым перехватывает события. При равных `z` сохраняется порядок в разметке. `z` не влияет на раскладку и действует локально для родителя.
- Построчные отступы `padding-top`/`padding-right`/`padding-bottom`/`padding-left` и `margin-top`/`margin-right`/`margin-bottom`/`margin-left`: переопределяют соответствующую сторону базовых `padding`/`margin`. Тип отступов — `relay::vdom::Edges` (`Style::padding`/`Style::margin`), а `Style::inset()` теперь возвращает отступы по сторонам.
- Выравнивание **содержимого** контейнера: `justify-content` (main-ось) и `align-items` (cross-ось) со значениями `start`/`center`/`end`. `Column`/`<window>` — main по вертикали, `Row`/`<titlebar>` — по горизонтали. `position-x`/`position-y` конкретного ребёнка переопределяет `align-items` родителя (поля `Style::justify_content`, `Style::align_items`).
- Динамические привязки `:attr` для **всех** атрибутов: значения стилей (`:width`, `:height`, `:min-*`/`:max-*`, `:position-x`/`:position-y`, `:position-offset-*`, `:justify-content`, `:align-items`, `:padding`, `:margin`, `:border-size`, `:font-size`, `:background`, `:font-color`, `:border-color`, `:required`, `:required-indicator`) и типов узлов (`:type` у `<button>`, `:placeholder`/`:textarea`/`:resize` у полей, `:value` у `<radio>` и `<option>`) разбираются во время выполнения по тем же правилам, что и статические, и имеют приоритет над одноимённым статическим атрибутом. Новые публичные парсеры: `relay::vdom::parse_size_or_auto`, `parse_offset`, `parse_align_x`/`parse_align_y`, `parse_justify`/`parse_align_items`, `parse_px`, `parse_color`, `parse_bool`, `parse_required_indicator`, `parse_button_type`, `parse_resize`.
- `diff` учитывает изменение атрибутов узла (`VElement::attrs`) и полей `NodeKind` без отдельной операции (`Radio::value`, `TextField::multiline`/`resize`): такой узел помечается изменившимся и перерисовывается.
- Гранулярные Cargo-фичи: код неиспользуемых элементов и подсистем вырезается из бинарника. Набор по умолчанию — `elements-basic`, `window`, `a11y-winit`; остальные возможности (элементы форм, `align-content`, `z-order`, `cursor-theming`, `offset-position`, `dynamic-attrs`, `required-fields`, `validation`, `min-max-size`, `window-*`, `a11y-*`, `include`) включаются явно. Сетка фич описана в [docs/DESIGN_PRINCIPLES.md](docs/DESIGN_PRINCIPLES.md) и [docs/guide.md](docs/guide.md).
- Понятная ошибка компиляции при использовании выключенного элемента: `<password>`/`<checkbox>`/`<radio>`/`<select>` без соответствующей фичи больше не превращаются в «неизвестный элемент» — макрос `#[template]` называет нужную фичу и подсказывает, как её включить.
- CLI `relay` (`crates/relay-cli`, ядро — `crates/relay-cli-core`) с командами `check` и `suggest`: статически сканирует `*.html`-шаблоны проекта, сопоставляет использованные теги/атрибуты/директивы со встроенным реестром компонентов и показывает отсутствующие и неиспользуемые компоненты. Вывод в формате `text`/`json`, флаг `--strict` для строгого кода возврата; только чтение, без запуска Cargo. Компилятор шаблонов вынесен в отдельный крейт `relay-html`.
- CLI: `relay doctor`, `relay add`, `relay remove`, `relay update`, `relay lock`, `relay vendor`.
- `relay.lock` for reproducible builds.
- Backups and automatic rollback for all CLI mutations.
- Workspace-component detection: `relay add` uses local path for workspace members.
- Команда `relay doctor [путь]` (`crates/relay-cli`, ядро — `relay-cli-core`): проверяет версию `cargo`, доступность crates.io (при включённой фиче `network`), установленные компоненты и их совместимость с версией ядра. Вывод в формате `text`/`json`, флаг `--offline`; код возврата `1` при ошибках.
- Команда `relay add <компонент> [путь]`: добавляет зависимость компонента в `Cargo.toml`, включает соответствующую фичу (имя компонента без префикса `relay-`, переопределяется `--feature`), обновляет `relay.lock` и проверяет сборку через `cargo check`. Команда `relay remove <компонент> [путь]` удаляет зависимость и вычищает связанную фичу. Список устанавливаемых зависимостей и фич выводится в виде плана.
- Команда `relay update [компонент] [путь]`: обновляет установленные компоненты до актуальных версий (для локальных — версия определяется крейтом, для crates.io — встроенным реестром), применяя изменения через `toml_edit` и фиксируя их в `relay.lock`.
- Команда `relay lock [путь]`: создаёт или обновляет `relay.lock` (точные версии и источник компонентов, версия ядра), печатает разницу с текущим состоянием и поддерживает `--dry-run`.
- Команда `relay vendor [путь]`: копирует исходники локальных компонентов в `vendor/` (исключая `target/` и `.git/`), чтобы проект собирался без сети и без соседних репозиториев. Компоненты, доступные только на crates.io, помечаются как пропущенные.
- Общие для изменяющих команд возможности: `--dry-run` (показать план), `--yes` (без вопросов), `--offline` (только workspace/vendor, без сети), `--no-verify` (не запускать `cargo check`). Источник компонента выбирается в порядке workspace → vendor → crates.io.
- Резервное копирование и откат: перед изменением `Cargo.toml`/`relay.lock` снимок сохраняется в `.relay/backup/`, а при неудачном `cargo check` файлы восстанавливаются (включая удаление появившихся файлов).
- Ядро `relay-cli-core` расширено модулями: `process` (запуск внешних команд и разбор версии), `cargo` (обёртки `cargo add`/`remove`/`check`), `manifest`/`edit` (чтение и правка `Cargo.toml` через `toml_edit`), `doctor`, `fetch` (разрешение источника компонента), `backup`, `lockfile` (создание/диф `relay.lock`), `add`/`remove`/`update`/`vendor`. Сетевой доступ (`reqwest`) подключается фичей `network` (у `relay-cli` включена).
- Тесты `relay-cli-core` (модульные, с `MockRunner`) и интеграционные тесты `relay-cli` на временных копиях фикстур (`tests/fixtures/`).
- Атрибут `required` для `<text>`/`<password>`/`<textarea>`/`<checkbox>`/`<select>` и групп `<radio>`; настраиваемый индикатор `required-indicator` (`*`, `none` или цвет).
- Пользовательские проверки: `:validator="метод"` у поля (`fn(&self, value: &str) -> Result<(), String>`) и `@validate="метод"` у формы (`fn(&self) -> Result<(), Vec<(String, String)>>`).
- Атрибут `type` у `<button>`: `submit` (по умолчанию), `button`, `reset`.
- Модуль `relay::validation`: `validate_required`, `validate_form`, `find_submit_button`, `ValidationResult`, `FieldError`, `form_ancestor`.
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
- `relay doctor` correctly detects `relay-core` version in workspace projects.
- `relay add` is idempotent for already-installed components (exit 0).
- `relay remove` updates `relay.lock` and does not drop unrelated empty features.
- `relay vendor --path` flag now parses correctly.

### Changed

- Монолит разбит на ядро и компоненты: код переехал в новый крейт `relay-core` (vdom, реактивность, раскладка, paint, окно, доступность), а крейт `relay` стал тонкой мета-обёрткой, которая реэкспортирует ядро, макросы и подключённые компоненты. Добавлены компоненты `relay-element-form` (элементы `<checkbox>`/`<radio>`/`<select>`/`<option>`) и `relay-behavior-validation` (`required`, `:validator`, `@validate`); фичи `relay/elements-form` и `relay/validation` тянут соответствующие компоненты. Текстовые поля (`<text>`, `<password>`, `<textarea>`) входят в ядро. Публичный API сохранён: `use relay::prelude::*;` и `#[template]`/`#[proxy]` работают без изменений.
- Встроенный реестр CLI приведён к новой структуре крейтов: удалён `relay-element-text` (текстовые поля — это ядро), версии компонентов — `0.2`, требование к ядру — `^0.2`.
- `<button>` стал контейнером: подпись собирается из содержимого детей (голый текст и вложенные элементы), выкладывается по горизонтали и центрируется внутри кнопки по обеим осям. Поле `NodeKind::Button::label` удалено; обновление подписи идёт через обычный дифф детей (`Op::SetContent`). Вертикальная компоновка содержимого задаётся вложенным `<div direction="vertical">`.
- `position-x`/`position-y` принимают значение `center` (выравнивание по центру контентной области родителя).
- Позиционирование и размеры-проценты отсчитываются от **контентной области родителя** (границы минус `padding` и рамка), а не от окна: `position-x`/`position-y`, `position-offset-x`/`position-offset-y` и проценты в `width`/`height`/`min-*`/`max-*`. База берётся от итогового размера родителя. В вертикальном потоке (`direction="vertical"`, `<window>`) контейнер без явного `width` растягивается на доступную ширину, в горизонтальном (`direction="horizontal"`, `<titlebar>`) — без явного `height` по высоте; растяжение отключается явным размером по этой оси. Где растяжение не применяется (например, ширина контейнера в горизонтальном потоке), `Auto`-база равна `0`, и проценты по этой оси дают `0`. Корень и `<window>` занимают всё окно.
- Текст внутри заблокированной (`:disabled`) кнопки рисуется приглушённым цветом.
- Split `relay` into `relay-core`, `relay-element-form`, `relay-behavior-validation`.
  Existing code using `relay` continues to work with default features.
- Gated elements (`<select>`, `<checkbox>`, `<radio>`) now produce a clear
  compile-time error when used without the corresponding feature.

### Notes

- Поддержку доступности можно выключить переменной окружения `RELAY_A11Y=0` или программно — `relay::a11y::set_enabled(false)`.

## [0.3.0] - 2026-10-20

### Added

- `docs/COMPONENT_AUTHORING.md` — практическое руководство по созданию
  и публикации компонентов Relay.
- Корневой `README.md` с кратким описанием, быстрым стартом и списком крейтов.
- Лицензии `LICENSE-MIT` и `LICENSE-APACHE` (двойное лицензирование MIT/Apache-2.0);
  копии добавлены в каталоги публикуемых крейтов, чтобы попадать в `.crate`.
- Метаданные crates.io для публикуемых крейтов: `description`, `license`,
  `repository` и `readme`.
- Команда `relay publish [крейт]`: публикует крейты воркспейса в crates.io в
  топологическом порядке (граф строится из `cargo metadata`), пропуская
  `publish = false`. Pre-flight гейты (`cargo build`/`test`/`clippy`/`fmt`,
  строгая проверка шаблонов, валидация `[package.metadata.relay]`, сверка версий
  внутренних зависимостей и проверка «уже опубликовано» через crates.io API и
  git-теги), ретраи на устаревание индекса crates.io с экспоненциальной задержкой,
  интерактивное подтверждение и флаги `--dry-run` (упаковка через
  `cargo package --list` без сети и без публикации), `--yes`, `--allow-dirty`,
  `--token`, `--verify`/`--no-verify`, `--skip-gates`, `--only`.
- Команда `relay init <имя>`: создаёт скелет нового проекта Relay — `Cargo.toml`
  с зависимостью `relay`, самодостаточный `src/main.rs` (окно с «Hello, Relay!»),
  `.gitignore` и `relay.lock`. Каталог задаётся позиционно (`./<имя>`) или флагом
  `--path` (имя крейта при этом остаётся позиционным). Флаг `--minimal` создаёт
  только `Cargo.toml` и `src/main.rs`. Ошибки, если каталог не пуст или `cargo`
  не найден в `PATH`.

### Fixed

- `relay add`/`relay remove` сохраняют форматирование `Cargo.toml`: соседние
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
  `wgpu`/`naga`. Фича `softbuffer-backend` поддерживает `relay` и
  `relay-core`, а демо получило отдельные `softbuffer`/`wgpu`/`full-wgpu`
  для сборки обоих вариантов.

### Changed

- Набор по умолчанию (`relay`, `relay-core`, `relay-demo`) использует
  `softbuffer-backend` вместо `wgpu-backend`. Release-сборка демо уменьшилась
  с 7.86 MB до 1.70 MB (−6.16 MB, −78%). Если включены обе фичи, окно
  выбирает `softbuffer`.

