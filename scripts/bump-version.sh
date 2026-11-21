#!/usr/bin/env bash
# Бамп версии всего воркспейса в один шаг (Linux/macOS; дубль bump-version.ps1).
#
# Обновляет функциональные поля `version = "X.Y.Z"` в Cargo.toml (workspace.package,
# [package], path-зависимости) и в Cargo.lock; комментарии и description не трогаем.
#
# Использование:  scripts/bump-version.sh <старая_версия> <новая_версия>
# Пример:         scripts/bump-version.sh 0.20.0 0.20.1

set -euo pipefail

if [[ $# -ne 2 ]]; then
    echo "Usage: $0 <old_version> <new_version>" >&2
    exit 2
fi
OLD="$1" NEW="$2"
if [[ "$OLD" == "$NEW" ]]; then
    echo "old и new совпадают" >&2
    exit 1
fi

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

mapfile -t FILES < <(
    printf 'Cargo.toml\n'
    printf 'Cargo.lock\n'
    find crates examples -name Cargo.toml -type f 2>/dev/null || true
)

total=0
for file in "${FILES[@]}"; do
    [[ -f "$file" ]] || continue
    # Заменяем version = "OLD" -> version = "NEW", но только на строках,
    # которые НЕ начинаются с '#' и НЕ являются `description = ...`.
    n=$(awk -v old="$OLD" '
        {
            line=$0; t=line; sub(/^[ \t]+/,"",t)
            if (t ~ /^#/) next
            if (t ~ /^description[ \t]*=/) next
            if (t ~ /version[ \t]*=[ \t]*"/) {
                idx=index(line,"version = \"" old)
                if (idx>0) { c++; }
            }
        } END { print c+0 }' "$file")
    if [[ "$n" -gt 0 ]]; then
        perl -i -pe 's/(version[ \t]*=[ \t]*")'"$OLD"'(")/${1}'"$NEW"'${2}/' "$file"
        printf '%-44s %d строк\n' "${file#./}" "$n"
        total=$((total+n))
    fi
done

if [[ "$total" -eq 0 ]]; then
    echo "Ничего не заменено: версия '$OLD' не найдена." >&2
    exit 1
fi

# Sanity-check: все 15 workspace-крейтов в lock имеют новую версию.
for pkg in light light-demo light-fw light-fw-behavior-validation \
           light-fw-cli-core light-fw-core light-fw-devtools-web \
           light-fw-element-form light-fw-html light-fw-macros light-gui \
           light-reactivity light-render-cpu light-render-gpu light-vdom; do
    grep -qzA1 "^name = \"$pkg\$" Cargo.lock >/dev/null 2>&1 || true
    if ! awk -v p="$pkg" -v v="$NEW" '
        /^name = "/ { p0=$0 }
        p0 == "name = \"" p "\"" { if ($0 ~ ("version = \"" v "\"")) { ok=1 } }
        END { exit ok ? 0 : 1 }' Cargo.lock; then
        echo "ОШИБКА: пакет '$pkg' без версии $NEW в Cargo.lock" >&2
        exit 1
    fi
done

echo ""
echo "Всего заменено: $total (версия $OLD -> $NEW)"
echo "Санити-чек: все 15 workspace-крейтов в Cargo.lock имеют $NEW. OK"
