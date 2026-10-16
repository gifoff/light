# Замер размера бинарника при разных наборах фич.
#
# Собирает один и тот же пример `minimal` в релизе с фичами `minimal`
# (элементы + окно), `a11y` (+ AccessKit) и `full` (+ формы, кросс-элементные,
# оконные возможности) и печатает размеры. Разница показывает вклад подсистем.
#
# Запуск из корня репозитория:
#     powershell -NoProfile -File scripts/bloat.ps1
# Если установлен cargo-bloat (`cargo install cargo-bloat`), печатается ещё и
# разбивка по крейтам/функциям.

$ErrorActionPreference = "Stop"

# Приближаем релиз к минимальному бинарнику (рекомендации из docs/guide.md),
# не меняя профиль в репозитории.
$env:CARGO_PROFILE_RELEASE_STRIP = "true"
$env:CARGO_PROFILE_RELEASE_LTO = "thin"
$env:CARGO_PROFILE_RELEASE_CODEGEN_UNITS = "1"

function Get-BinaryPath {
    $base = Join-Path "target" "release\examples\minimal"
    if (Test-Path "$base.exe") { return "$base.exe" }
    return $base
}

function Invoke-Build([string]$Features) {
    & cargo build -p relay-demo --example minimal --release --no-default-features --features $Features
    if ($LASTEXITCODE -ne 0) { throw "Сборка (features=$Features) не удалась" }
    return (Get-Item (Get-BinaryPath)).Length
}

$sizes = [ordered]@{}
foreach ($feature in @("minimal", "a11y", "full")) {
    $sizes[$feature] = Invoke-Build $feature
}

"Размер бинарника (пример minimal, release + strip):"
$base = $sizes["minimal"]
foreach ($feature in $sizes.Keys) {
    $size = $sizes[$feature]
    $delta = $size - $base
    "  {0,-8} {1,10:N0} байт  (+{2:N0})" -f $feature, $size, $delta
}

if (Get-Command cargo-bloat -ErrorAction SilentlyContinue) {
    "`ncargo bloat (minimal):"
    & cargo bloat --release -p relay-demo --example minimal --no-default-features --features minimal -n 20
}
