# Модель компонентов Light

Этот документ фиксирует архитектурную модель компонентов Light: что считается компонентом, какие бывают типы, как они описываются, распространяются и собираются в приложение.

Связанные документы:
- `docs/DESIGN_PRINCIPLES.md` — общие принципы (в частности, «только используемый код попадает в бинарник»).
- `docs/guide.md` — руководство пользователя.
- `docs/COMPONENT_AUTHORING.md` — как писать собственные компоненты.

---

## 1. Что такое компонент

**Компонент** — это автономная единица функциональности Light, которая:

1. Имеет **манифест** с именем, версией, типом и совместимостью с ядром.
2. Распространяется через **стандартный реестр** (crates.io или приватный mirror) либо как **asset pack** (архив данных).
3. Может быть **включён или выключен** в конкретном приложении — код выключенного компонента не попадает в бинарник.
4. Объявляет, **что предоставляет** (`provides`): элементы, атрибуты, директивы, API-функции.
5. Объявляет, **что требует** (`requires`): другие компоненты и совместимую версию ядра.
6. Следует правилам из `DESIGN_PRINCIPLES.md`.

Компонент — это не «плагин во время выполнения». Это **compile-time единица**: решение о включении принимается один раз, при сборке.

---

## 2. Принципы

1. **Ядро минимально.** Сам фреймворк — это core (vdom, reactive, layout, paint, окно). Всё остальное — компоненты.
2. **Один компонент — одна ответственность.** Не бывает «мега-компонента» с checkbox, dropdown и графиком внутри.
3. **Зависимости явные.** Компонент не может «неявно» требовать другой компонент. Все требования — в манифесте.
4. **Совместимость — по semver.** Без своих версий и «совместимо со всем».
5. **Собирается только используемое.** См. `DESIGN_PRINCIPLES.md`.
6. **Cargo остаётся движком.** Компоненты-крейты собираются Cargo.
7. **Данные отдельно от кода.** Темы, иконки, локали — asset packs, не крейты.

---

## 3. Таксономия компонентов

Компоненты делятся на **восемь типов**.

### 3.1. Core (ядро)
Не компонент, а **сам фреймворк**. `light-fw-core`, `light-fw-macros`. Минимальный набор: виртуальный DOM, реактивность, базовая раскладка, окно и paint. Ядро **не может быть выключено**.

Бесплатно в ядре живут базовые элементы: `<div>`, `<span>`, `<h1>`–`<h5>`, `<titlebar>`, `<window>`, `<button>`, а также текстовые поля `<text>`, `<password>` и `<textarea>`.

### 3.2. Element (элемент)
Примитивные сущности разметки: теги, доступные в HTML-шаблонах. Один крейт = одна логическая группа элементов.
- `light-fw-element-form` — `<checkbox>`, `<radio>`, `<select>`, `<option>`.

### 3.3. Behavior (поведение)
Не создают новых элементов. Добавляют функциональность существующим: обработку событий, свойства, модификацию раскладки.
- `light-fw-behavior-validation` — валидация форм (`:validator`, `@validate`, `required`).

### 3.4. Widget (виджет)
Сложные составные компоненты, собранные из elements + behaviors. Предоставляют готовый к использованию UI-элемент.
- `light-widget-datepicker`, `light-widget-autocomplete`.

### 3.5. Integration (интеграция)
Мост с внешним миром: ОС, устройства, сторонние сервисы.
- `light-integration-tray`, `light-integration-clipboard`, `light-integration-filedialog`.

### 3.6. Subsystem (подсистема)
Крупные блоки, меняющие поведение всего фреймворка или большого его слоя.
- `light-subsystem-i18n`, `light-subsystem-theme`, `light-subsystem-devtools`.

### 3.7. Data (данные)
Не код. Пакет файлов: темы, иконки, локали, шрифты, звуки. Формат — стандартный zip с `manifest.toml` внутри. Не содержит нативного кода.

### 3.8. Dev-only (для разработки)
Не попадает в релизную сборку. Используется во время разработки.
- `light-devtools`, `light-hot-reload`. Всегда под фичей, никогда не в `default`.

---

## 4. Манифест компонента (крейт)

Компоненты-крейты используют **стандартный `Cargo.toml`** плюс секцию `[package.metadata.light]`.

```toml
[package]
name = "light-widget-datepicker"
version = "0.1.0"
edition = "2021"
rust-version = "1.95"
license = "MIT OR Apache-2.0"
description = "Календарь для выбора даты"
repository = "https://github.com/user/light-widget-datepicker"

[dependencies]
light-fw = { version = "0.17", default-features = false, features = ["elements-basic", "elements-form"] }
light-fw-element-form = { version = "0.17" }

[package.metadata.light]
# Тип компонента: element | behavior | widget | integration | subsystem | dev
kind = "widget"
display-name = "Date Picker"
# Совместимость с ядром — semver-range
core = "^0.17"

[package.metadata.light.provides]
elements = ["datepicker"]              # новые теги
attributes = []                        # новые атрибуты
directives = []                        # новые директивы (v-*)

[package.metadata.light.requires]
# Компоненты, которые обязательны
components = [
    "light-fw-element-form ^0.17",
]

[package.metadata.light.registry]
official = false
tags = ["form", "date", "input", "calendar"]
```

**Важно**: Для использования DevTools и горячей перезагрузки в проекте пользователя должна быть включена фича `dev-mode` (она транслируется в `light-fw/dev-mode`), а в `main.rs` должен использоваться `light_fw::run_with_config("light.config.json", App::new())`.
