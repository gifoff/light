# REPORT-release-0.19.0: Публикация версии 0.19.0

Дата: 2026-11-16
Статус: **Все 9 крейтов опубликованы на crates.io, CI зелёный**

---

## Bump-коммиты

| Коммит | Описание | Ссылка |
|--------|----------|--------|
| `52c9211` | chore(release): bump to 0.19.0 | [52c9211](https://github.com/gifoff/light/commit/52c9211) |
| `74ad2b4` | docs: update changelog for 0.19.0 | [74ad2b4](https://github.com/gifoff/light/commit/74ad2b4) |
| `267955d` | Merge release/0.19.0 into master | [267955d](https://github.com/gifoff/light/commit/267955d) |
| `4e5a734` | fix(macros): remove circular dev-dependency | [4e5a734](https://github.com/gifoff/light/commit/4e5a734) |
| `3f7305a` | fix(test): correct trybuild .stderr (final) | [3f7305a](https://github.com/gifoff/light/commit/3f7305a) |

---

## CI-раны

| Run | Статус | Описание |
|-----|--------|----------|
| [43923653357](https://github.com/gifoff/light/actions/runs/43923653357) | ✅ 15/15 | master после merge (74ad2b4) |
| [43926480438](https://github.com/gifoff/light/actions/runs/43926480438) | ✅ success | Deploy documentation (267955d) |
| [43932455103](https://github.com/gifoff/light/actions/runs/43932455103) | ❌ | После 4e5a734 — path depth bug |
| [43939739245](https://github.com/gifoff/light/actions/runs/43939739245) | ✅ **15/15** | Финальный (3f7305a) |

---

## Публикация на crates.io

Все 9 крейтов опубликованы в порядке из `docs/RELEASING.md`:

| # | Крейт | Версия | Статус |
|---|-------|--------|--------|
| 1 | light-fw-html | 0.19.0 | ✅ |
| 2 | light-fw-cli-core | 0.19.0 | ✅ |
| 3 | light-fw-devtools-web | 0.19.0 | ✅ |
| 4 | light-fw-macros | 0.19.0 | ✅ |
| 5 | light-fw-core | 0.19.0 | ✅ |
| 6 | light-fw-behavior-validation | 0.19.0 | ✅ |
| 7 | light-fw-element-form | 0.19.0 | ✅ |
| 8 | light-fw | 0.19.0 | ✅ |
| 9 | light | 0.19.0 | ✅ |

Вывод `cargo search light-fw` (после публикации):

```
light-fw = "0.19.0"                        # Реактивный графический интерфейс для Rust
light-fw-html = "0.19.0"                   # Разбор HTML-шаблонов Light для процедурных макросов
light-fw-macros = "0.19.0"                 # Процедурные макросы библиотеки Light: #[proxy] и #[template]
light-fw-core = "0.19.0"                   # Ядро Light: виртуальный DOM, реактивность, раскладка, отрисовка и окно
light-fw-cli-core = "0.19.0"              # Ядро CLI Light: анализ проекта, управление компонентами и light.lock
light-fw-behavior-validation = "0.19.0"   # Компонент Light: валидация форм
light-fw-element-form = "0.19.0"          # Компонент Light: элементы формы
light = "0.19.0"                          # Утилита light
light-fw-devtools-web = "0.19.0"          # Web UI DevTools: браузерный инспектор VDOM/State
```

---

## GitHub Release

**Не создан.** Токен `GH_TOKEN` не имеет scope `repo:write` (HTTP 403 на `POST /releases`).

Готовая команда (нужен токен с `repo:write`):

```bash
gh release create v0.19.0 \
  --title "Release 0.19.0: Plugin API v1" \
  --notes "## Plugin API v1

### Added
- #[component] — макрос для объявления пользовательских элементов
- light-fw-element-form — новый крейт с формой и валидацией
- light check — проверка проекта на использование неизвестных тегов
- light dev --agent — headless-инструменты для тестирования UI
- light devtools shot/find/click/type/key/state/tree — DevTools CLI
- Web UI на http://127.0.0.1:9228 при запуске light dev
- Центрирование h1–h5 в align-items=center
- trybuild UI-тест для ошибок неизвестных тегов

### Fixed
- Циклическая dev-зависимость light-fw-macros → light-fw
- Путь trybuild fixture (4 уровня, не 5)
- UTF-8 BOM в .stderr файлах
- Индентация .stderr для двузначных номеров строк

### Публикуемые крейты
light-fw-html, light-fw-cli-core, light-fw-devtools-web,
light-fw-macros, light-fw-core, light-fw-behavior-validation,
light-fw-element-form, light-fw, light — все 0.19.0" \
  --latest
```

Тег `v0.19.0` уже создан и опубликован (указывает на `267955d`).

---

## Пути к отчётам

- Этот файл: `REPORT-release-0.19.0.md`
- Детальный отчёт по фиксу публикации: [`REPORT-publish-fix.md`](REPORT-publish-fix.md)
