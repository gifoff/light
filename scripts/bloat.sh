#!/usr/bin/env bash
# Замер размера бинарника при минимальном и полном наборах фич.
#
# Собирает один и тот же пример `minimal` (только базовые элементы) в релизе
# с фичами `minimal` и `full` и печатает размеры. Разница показывает, сколько
# кода опциональных подсистем попадает в бинарник при полном наборе.
#
# Запуск из корня репозитория:
#     bash scripts/bloat.sh
# Если установлен cargo-bloat (`cargo install cargo-bloat`), печатается ещё и
# разбивка по крейтам/функциям.
set -euo pipefail

binary_path() {
    if [[ -f target/release/examples/minimal.exe ]]; then
        echo target/release/examples/minimal.exe
    else
        echo target/release/examples/minimal
    fi
}

build_size() {
    local features="$1"
    cargo build -p relay-demo --example minimal --release --no-default-features --features "$features"
    stat -c '%s' "$(binary_path)" 2>/dev/null || stat -f '%z' "$(binary_path)"
}

minimal=$(build_size minimal)
full=$(build_size full)

echo "Размер бинарника (пример minimal):"
printf '  minimal: %8d байт\n' "$minimal"
printf '  full:    %8d байт\n' "$full"
printf '  разница: %8d байт\n' "$((full - minimal))"

if command -v cargo-bloat >/dev/null 2>&1; then
    echo
    echo "cargo bloat (minimal):"
    cargo bloat --release -p relay-demo --example minimal --no-default-features --features minimal -n 20
fi
