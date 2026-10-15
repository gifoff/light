#!/usr/bin/env bash
# Проверка сборки и тестов relay на разных наборах Cargo-фич.
#
# Ловит забытые `#[cfg]`: набор должен компилироваться и проходить тесты
# не только на `--all-features`. Запуск из корня репозитория:
#     bash scripts/check-features.sh
set -euo pipefail

combos=(
    "без фич|--no-default-features"
    "базовые элементы|--no-default-features --features elements-basic"
    "базовые + окно + a11y|--no-default-features --features elements-basic,window,a11y-winit"
    "формы + окно + a11y|--no-default-features --features elements-form,window,a11y-winit"
    "a11y без клавиатуры|--no-default-features --features elements-basic,window,a11y"
    "кросс-элементные|--no-default-features --features elements-form,window,a11y-winit,include,align-content,z-order,cursor-theming,offset-position,dynamic-attrs,required-fields,validation,min-max-size"
    "все фичи|--all-features"
)

for combo in "${combos[@]}"; do
    name="${combo%%|*}"
    args="${combo#*|}"
    echo "==> ${name}"
    # shellcheck disable=SC2086
    cargo test -p relay ${args}
done

echo "Все наборы фич зелёные."
