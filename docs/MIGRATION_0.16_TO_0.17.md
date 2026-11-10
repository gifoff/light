# Переход с Light 0.16.x на 0.17.x

В версии 0.17 переименованы крейты CLI и библиотеки для однозначного
разделения ответственности:

| Было (0.16.x) | Стало (0.17.x) | Назначение |
|---|---|---|
| `light` (обёртка) | `light-fw` | Мета-библиотека (re-exports ядра + фичи компонентов) |
| `light-cli` | `light` | Командная строка (`cargo install light`) |
| `light-core` | `light-fw-core` | Ядро (VDOM, layout, paint, runtime, DevTools) |
| `light-macros` | `light-fw-macros` | Макросы `#[template]`, `#[proxy]` |
| `light-html` | `light-fw-html` | HTML-парсер |

## Что меняется в вашем проекте

### 1. `Cargo.toml`

Замените зависимости:

```toml
# Было
[dependencies]
light = { version = "0.16" }

# Стало
[dependencies]
light-fw = { version = "0.17" }
```

Фичи тоже переименованы:

```toml
# Было
[features]
dev-mode = ["light/dev-mode"]

# Стало
[features]
dev-mode = ["light-fw/dev-mode"]
```

Компоненты:

```toml
# Было
light-element-form = { version = "0.16" }

# Стало
light-fw-element-form = { version = "0.17" }
```

### 2. Код Rust

Все пути `light::` заменяются на `light_fw::`:

```rust
// Было
use light::prelude::*;
light::run_with_config("light.config.json", App::new());
let token = light::current_window_token();

// Стало
use light_fw::prelude::*;
light_fw::run_with_config("light.config.json", App::new());
let token = light_fw::current_window_token();
```

### 3. Документация / ссылки

Ссылки `[`light::...`]` → `[`light_fw::...`]`.

## Чего НЕ нужно менять

- **Старые опубликованные проекты** продолжают работать: `light` (обёртка)
  осталась доступной на crates.io. Новые проекты генерируются с `light-fw`.
- **Каталог крейтов**: `crates/light` теперь содержит CLI (bin `light`),
  `crates/light-fw` — библиотеку.
- **CLI-интерфейс**: команды те же, добавлена `light build`.

## Автоматическая проверка

CLI автоматически обнаруживает недостающие компоненты перед `light dev`
и `light build`. Если вы используете `<select>` без `light-fw-element-form`,
CLI спросит:

```
⚠️ Обнаружены недостающие компоненты:
   - light-fw-element-form (для <select>)
Установить их автоматически? (Y/n):
```

Ответ `Y` установит компонент автоматически.

## Проверка совместимости

После обновления запустите:

```bash
light check --strict
```

Эта команда сопоставит используемые теги/атрибуты с установленными
компонентами и укажет на несоответствия.

## Чеклист миграции

- [ ] Заменить `light` → `light-fw` в `Cargo.toml`
- [ ] Заменить `light/...` → `light-fw/...` в секции `[features]`
- [ ] Заменить все `light::` → `light_fw::` в `.rs` файлах
- [ ] Заменить ссылки в документации
- [ ] Запустить `cargo build`
- [ ] Запустить `light check --strict`
- [ ] Запустить `cargo test`
