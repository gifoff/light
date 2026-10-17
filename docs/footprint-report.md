# Footprint Report

Дата: 2026-10-17
Commit: 525fa05 (Фаза 1: split `relay` → `relay-core` + компоненты)

## relay-demo (release, strip)

| Конфигурация | Пример | Размер `.exe` | relay_core |
|---|---:|---:|---:|
| minimal (без форм и a11y) | `minimal` | 8 074 752 B (7.7 MB) | 304.5 KiB |
| full (формы + a11y) | `a11y_form` | 8 198 656 B (7.8 MB) | 315.4 KiB |

## Эффект разделения на компоненты

- **Дельта `relay_core`**: ~11 KB (код формы и валидации).
- **Общая дельта**: ~124 KB (включает код примера и `serde_json` для `--dump-a11y`).

## Узкие места

1. **wgpu (~3.8 MB)** — тянется через `pixels` и транзитивно `wgpu_core` + `naga`.
   План — исследование CPU-рендеринга в 0.4 (см. `docs/ROADMAP.md`).
2. **accesskit_windows (~171 KiB)** + `accesskit_consumer` (~139 KiB) —
   доступность, включена по умолчанию.
3. **std (~609 KiB)** — базовый Rust, не поддаётся оптимизации.
