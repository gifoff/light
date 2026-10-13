# Changelog

Все заметные изменения проекта. Формат основан на [Keep a Changelog](https://keepachangelog.com/ru/1.1.0/), версии — по [Semantic Versioning](https://semver.org/lang/ru/).

## [Unreleased]

### Added

- Доступность (a11y) на базе AccessKit: сопоставление ролей, `aria-*`-атрибуты и построение дерева доступности (`relay::a11y`).
- Интеграция с `accesskit_winit`: адаптер окна, отправка `TreeUpdate` при перерисовке и обработка `ActionRequest` от диктора.
- Управление фокусом и клавиатурная навигация: `Tab`/`Shift+Tab`, активация `Enter`/`Space`.
- Примеры `a11y_form` и `a11y_dialog` (`cargo run --example …`).
- Фича `a11y-winit` (включена по умолчанию вместе с `window` и `a11y`).
- Проверка статического `tabindex` на этапе компиляции: нечисловое значение — ошибка сборки (динамический `:tabindex` проверяется во время выполнения).
- Курсоры мыши: автоматический выбор по роли узла (ссылки и элементы управления — `pointer`, поля ввода — `text`, отключённые — `not-allowed`, остальные — `default`), переопределение атрибутом `cursor`/`:cursor` и всплытие к ближайшему подходящему родителю (`relay::vdom::Cursor`, `relay::window::resolve_cursor`).
- Проверка статического `cursor` на этапе компиляции: неизвестное имя — ошибка сборки (динамический `:cursor` проверяется во время выполнения).
- Режим `--dump-a11y` в примерах `a11y_form` и `a11y_dialog`: печатает дерево доступности в JSON.

### Fixed

- `TreeUpdate.focus` теперь указывает на первый tabbable-узел, если явный фокус не задан (раньше — на корень).
- `placeholder` текстового поля используется как доступное имя (`label`), если не задан `aria-label`/`aria-labelledby`.
- Контейнеры больше не получают `label`, собранный из текста потомков: имя выводится из содержимого только для семантических ролей (`Button`, `Label`, `Link`, `Heading`, `CheckBox`, `RadioButton`, `MenuItem`).
- Отключённые узлы (`disabled`/`aria-disabled`) не получают события: клик по ним не вызывает `@click` и не всплывает к родителям, а курсор над ними — `not-allowed`.
- Клавиатурная активация (`Enter`/`Space`) отключённого узла игнорируется; узел, ставший отключённым в фокусе, уступает фокус следующему tabbable-элементу.

### Added (валидация форм)

- Атрибут `required` для `<text>`/`<password>`/`<textarea>`/`<checkbox>`/`<select>` и групп `<radio>`; настраиваемый индикатор `required-indicator` (`*`, `none` или цвет).
- Пользовательские проверки: `:validator="метод"` у поля (`fn(&self, value: &str) -> Result<(), String>`) и `@validate="метод"` у формы (`fn(&self) -> Result<(), Vec<(String, String)>>`).
- Атрибут `type` у `<button>`: `submit` (по умолчанию), `button`, `reset`.
- Модуль `relay::validation`: `validate_required`, `validate_form`, `find_submit_button`, `ValidationResult`, `FieldError`, `form_ancestor`.
- Показ ошибок: красная рамка и `aria-description` у невалидных полей, фокус на первом невалидном поле, озвучивание сообщения диктором; ошибка снимается при изменении значения.
- Отправка формы клавишей `Enter` из однострочного поля (`<text>`/`<password>`) внутри `role="form"`: срабатывает первая включённая кнопка `type="submit"`.
- Новые поля `Style`: `required`, `invalid`, `required_indicator`; новое поле `NodeKind::Button`: `button_type`. Стили учитываются в `diff`, поэтому смена валидности перерисовывает узел.
- Тесты `tests/validation.rs`, `tests/events.rs`, `tests/form_submit.rs` и проверки доступности `required`/`invalid`/`aria-description` в `tests/a11y_tree.rs`.

### Notes

- Поддержку доступности можно выключить переменной окружения `RELAY_A11Y=0` или программно — `relay::a11y::set_enabled(false)`.
