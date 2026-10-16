# Проверка сборки и тестов relay на разных наборах Cargo-фич.
#
# Ловит забытые `#[cfg]`: набор должен компилироваться и проходить тесты
# не только на `--all-features`. Запуск из корня репозитория:
#     pwsh scripts/check-features.ps1

$ErrorActionPreference = "Stop"

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
    & cargo test -p relay @($combo.Args)
    if ($LASTEXITCODE -ne 0) {
        throw "Набор фич «$($combo.Name)» завершился с ошибкой"
    }
}

Write-Host "Все наборы фич зелёные." -ForegroundColor Green
