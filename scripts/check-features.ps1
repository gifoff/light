# Проверка сборки и тестов light на разных наборах Cargo-фич.
#
# Ловит забытые `#[cfg]`: набор должен компилироваться и проходить тесты
# не только на `--all-features`. Запуск из корня репозитория:
#     pwsh scripts/check-features.ps1

$ErrorActionPreference = "Stop"

# Windows/MSVC: ring (через reqwest в light-fw-cli) не помещается в лимит
# командной строки D8050 при параллельной сборке — ограничиваем -j.
$env:CARGO_BUILD_JOBS = "1"

$combos = @(
    @{ Name = "без фич";                    Args = @("--no-default-features") },
    @{ Name = "базовые элементы";           Args = @("--no-default-features", "--features", "elements-basic") },
    @{ Name = "базовые + окно + a11y";      Args = @("--no-default-features", "--features", "elements-basic,window,a11y-winit") },
    @{ Name = "формы + окно + a11y";        Args = @("--no-default-features", "--features", "elements-form,window,a11y-winit") },
    @{ Name = "a11y без клавиатуры";        Args = @("--no-default-features", "--features", "elements-basic,window,a11y") },
    @{ Name = "кросс-элементные";           Args = @("--no-default-features", "--features", "elements-form,window,a11y-winit,include,align-content,z-order,cursor-theming,offset-position,dynamic-attrs,required-fields,validation,min-max-size") },
    @{ Name = "все фичи";                   Args = @("--all-features") }
)

foreach ($combo in $combos) {
    Write-Host "==> $($combo.Name)" -ForegroundColor Cyan
    foreach ($pkg in @("light-fw-core", "light")) {
        Write-Host "    -> $pkg" -ForegroundColor DarkCyan
        & cargo test -p $pkg @($combo.Args)
        if ($LASTEXITCODE -ne 0) {
            throw "Набор фич «$($combo.Name)» ($pkg) завершился с ошибкой"
        }
    }
}

# Примеры должны собираться БЕЗ --all-features (дефолтные фичи): это ловит
# регрессии 0.15.0, когда `#[template]` генерировал методы/реэкспорты,
# доступные только при `devtools`.
foreach ($example in @("example-counter", "example-dashboard", "example-multi-window", "example-devtools-demo")) {
    Write-Host "==> default build: $example" -ForegroundColor Cyan
    & cargo build -p $example
    if ($LASTEXITCODE -ne 0) {
        throw "Пример $example не собирается с дефолтными фичами"
    }
}

Write-Host "Все наборы фич зелёные." -ForegroundColor Green
