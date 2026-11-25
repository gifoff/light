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

### С версии 0.21.0 (рекомендуемый способ — для AI-агентов)
CLI сам спавнит процесс в фоне, печатает PID и сразу возвращает управление
терминалу. **Никакого PowerShell / Start-Process / win32 API не нужно.**

```bash
# 1) Фоновый запуск (блокирует 500 мс, затем терминал свободен):
light dev --agent --detach
# stdout (пример):
# {"ok":true,"pid":12345,"devtools_port":9229,"web_port":9228,"log":".light/agent-sessions/dev-detach.log"}
# PID: 12345
# DevTools port: 9229
# Web UI: http://127.0.0.1:9228/
# Log: .light/agent-sessions/dev-detach.log
# Остановка: light devtools session stop   (или taskkill /PID 12345 на Windows)
```

Подожди 3–5 секунд (пока приложение соберётся и окно откроется). Затем:

```bash
light devtools shot --path shots           # скриншот
light devtools find --tag button --text "Click"   # найти элемент
light devtools move --key tip_trigger      # @mouseenter (подсветка тупика)
light devtools click --key <key>           # клик
light devtools type --key <key> "text"     # ввод текста
light devtools key --key <key> Enter       # клавиша
light devtools state                       # JSON-состояние
light devtools tree                        # дерево элементов
```

Завершение сессии (сгенерирует `summary.md`):

```bash
light devtools session stop
```

Если по какой-то причине `session stop` недоступен (приложение упало),
останови процесс напрямую по PID:

```bash
taskkill /F /PID <pid>        # Windows
kill -9 <pid>                 # Linux/macOS
```

### Старый способ (≤ 0.20.x, только если нет `--detach`)
```powershell
Start-Process powershell -ArgumentList "-NoExit", "-Command", "cd .; light dev --agent"
```
Далее шаги те же, что выше (pодождать, `light devtools`, `session stop`).

## 3. Документация
Полное руководство: https://github.com/gifoff/light/blob/master/docs/guide.md
Локальные копии: `docs/guide.md` и `docs/cli.md` в корне этого проекта.
</content>
</invoke>
