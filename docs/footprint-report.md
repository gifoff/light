# Footprint Report

Дата: 2026-10-17
Commit: 525fa05 (Фаза 1: split `relay` → `relay-core` + компоненты)

## relay-demo (release, strip)

| Конфигурация | Пример | Размер `.exe` | relay_core |
|---|---:|---:|---:|
| minimal (без форм и a11y) | `minimal` | 8 074 752 B (7.7 MB) | 304.5 KiB |
| full (формы + a11y) | `a11y_form` | 8 198 656 B (7.8 MB) | 315.4 KiB |

## После перехода на softbuffer (2026-10-20, 0.4)

| Конфигурация | Бэкенд | Размер `.exe` |
|---|---|---:|
| full | `softbuffer` | 1 786 368 B (1.70 MB) |
| full-wgpu | `pixels`/`wgpu` | 8 242 688 B (7.86 MB) |

Переход на `softbuffer` уменьшил release-бинарник демо на 6 456 320 B
(−78%). `wgpu`/`naga` больше не входят в сборку по умолчанию.

## Эффект разделения на компоненты

- **Дельта `relay_core`**: ~11 KB (код формы и валидации).
- **Общая дельта**: ~124 KB (включает код примера и `serde_json` для `--dump-a11y`).

## Узкие места

1. **wgpu (~3.8 MB)** — тянется через `pixels` и транзитивно `wgpu_core` + `naga`.
   Исключён из сборки по умолчанию; остаётся в `wgpu-backend` для сравнения
   (см. `docs/ROADMAP.md`).
2. **accesskit_windows (~171 KiB)** + `accesskit_consumer` (~139 KiB) —
   доступность, включена по умолчанию.
3. **std (~609 KiB)** — базовый Rust, не поддаётся оптимизации.
