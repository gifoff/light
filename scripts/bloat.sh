#!/usr/bin/env bash
# Замер размера бинарника при разных наборах фич.
#
# Собирает один и тот же пример `minimal` в релизе с фичами `minimal`
# (элементы + окно), `a11y` (+ AccessKit) и `full` (+ формы, кросс-элементные,
# оконные возможности) и печатает размеры. Разница показывает вклад подсистем.
#
# Запуск из корня репозитория:
#     bash scripts/bloat.sh
# Если установлен cargo-bloat (`cargo install cargo-bloat`), печатается ещё и
# разбивка по крейтам/функциям.
set -euo pipefail

# Приближаем релиз к минимальному бинарнику (рекомендации из docs/guide.md),
# не меняя профиль в репозитории.
export CARGO_PROFILE_RELEASE_STRIP=true
export CARGO_PROFILE_RELEASE_LTO=thin
export CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1

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

declare -A sizes
for feature in minimal a11y full; do
    sizes[$feature]=$(build_size "$feature")
done

echo "Размер бинарника (пример minimal, release + strip):"
base=${sizes[minimal]}
for feature in minimal a11y full; do
    size=${sizes[$feature]}
    printf '  %-8s %10d байт  (+%d)\n' "$feature" "$size" "$((size - base))"
done

if command -v cargo-bloat >/dev/null 2>&1; then
    echo
    echo "cargo bloat (minimal):"
    cargo bloat --release -p relay-demo --example minimal --no-default-features --features minimal -n 20
fi
