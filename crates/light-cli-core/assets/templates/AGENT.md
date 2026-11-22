# Инструкция для AI-агентов (Light Framework)

Привет, AI! Если ты читаешь это, значит, ты работаешь над проектом
на Light. **НЕ ЧИТАЙ ИСХОДНИКИ ФРЕЙМВОРКА** (light-vdom, light-macros
и т.д.). Вся необходимая информация есть здесь.

## 1. Синтаксис шаблонов (Важно!)
- `<text>` — это **поле ввода** (требует `v-model`). Он не может
  содержать дочерних элементов. Для простого текста используй
  `<div>` или `<span>`: `<div font-size="24">{{ count }}</div>`.
- `<image src="path" width="..." height="..." />` — требует фичи
  `image-support` в Cargo.toml.
- Условный рендеринг: `<div v-if="condition">...</div>`.
  Условие — это имя метода или поля (например, `is_home`).

## 2. Как запустить и протестировать (DevTools)
**НИКОГДА** не используй `cargo run` и не блокируй терминал!

1. Запусти в фоне (PowerShell):
   `Start-Process powershell -ArgumentList "-NoExit", "-Command", "cd .; light dev --agent"`
2. Подожди 5-10 секунд (пока окно откроется).
3. Используй команды `light devtools`:
   - `light devtools shot --path shots` (сделать скриншот)
   - `light devtools find --tag button --text "Click"` (найти элемент)
   - `light devtools click --key <key>` (кликнуть)
   - `light devtools state` (посмотреть JSON-состояние)
   - `light devtools type --key <key> "text"` (ввести текст)
4. Останови сессию: `light devtools session stop`.

## 3. Документация
Полное руководство: https://github.com/gifoff/light/blob/master/docs/guide.md
Локальные копии: `docs/guide.md` и `docs/cli.md` в корне этого проекта.
</content>
</invoke>
