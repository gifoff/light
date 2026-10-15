# Замер размера бинарника при минимальном и полном наборах фич.
#
# Собирает один и тот же пример `minimal` (только базовые элементы) в релизе
# с фичами `minimal` и `full` и печатает размеры. Разница показывает, сколько
# кода опциональных подсистем попадает в бинарник при полном наборе.
#
# Запуск из корня репозитория:
#     pwsh scripts/bloat.ps1
# Если установлен cargo-bloat (`cargo install cargo-bloat`), печатается ещё и
# разбивка по крейтам/функциям.

$ErrorActionPreference = "Stop"

function Get-BinaryPath {
    $base = Join-Path "target" "release\examples\minimal"
    if (Test-Path "$base.exe") { return "$base.exe" }
    return $base
}

function Invoke-Build([string[]]$Features) {
    $featureArg = if ($Features.Count -eq 0) { "minimal" } else { $Features -join "," }
    $args = @("-p", "relay-demo", "--example", "minimal", "--release", "--no-default-features", "--features", $featureArg)
    & cargo build @args
    if ($LASTEXITCODE -ne 0) { throw "Сборка (features=$featureArg) не удалась" }
    return (Get-Item (Get-BinaryPath)).Length
}

$minimal = Invoke-Build @("minimal")
$full = Invoke-Build @("full")

"Размер бинарника (пример minimal):"
"  minimal: {0,8:N0} байт" -f $minimal
"  full:    {0,8:N0} байт" -f $full
$delta = $full - $minimal
"  разница: {0,8:N0} байт ({1:P1})" -f $delta, ($delta / $minimal)

if (Get-Command cargo-bloat -ErrorAction SilentlyContinue) {
    "`ncargo bloat (minimal):"
    & cargo bloat --release -p relay-demo --example minimal --no-default-features --features minimal -n 20
}
