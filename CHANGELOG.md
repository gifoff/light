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
- Режим `--dump-a11y` в примерах `a11y_form` и `a11y_dialog`: печатает дерево доступности в JSON.

### Fixed

- `TreeUpdate.focus` теперь указывает на первый tabbable-узел, если явный фокус не задан (раньше — на корень).
- `placeholder` текстового поля используется как доступное имя (`label`), если не задан `aria-label`/`aria-labelledby`.
- Контейнеры больше не получают `label`, собранный из текста потомков: имя выводится из содержимого только для семантических ролей (`Button`, `Label`, `Link`, `Heading`, `CheckBox`, `RadioButton`, `MenuItem`).

### Notes

- Поддержку доступности можно выключить переменной окружения `RELAY_A11Y=0` или программно — `relay::a11y::set_enabled(false)`.
