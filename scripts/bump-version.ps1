# Бамп версии всего воркспейса в один шаг (бесполезно вручную править 20 файлов).
#
# Обновляет ТОЛЬКО «функциональные» поля:
#   - `[workspace.package] version` и явные `[package] version` в Cargo.toml;
#   - `path = "...", version = "X.Y.Z"` в [workspace.dependencies];
#   - `version = "X.Y.Z"` у каждого пакета в Cargo.lock (workspace-крейты).
# Исторические упоминания версий в комментариях и description НЕ трогаем: они
# живут в строках, начинающихся с `#`, либо после слова `description`.
#
# Использование:
#   pwsh scripts/bump-version.ps1 <старая_версия> <новая_версия>
# Пример:
#   pwsh scripts/bump-version.ps1 0.20.0 0.20.1
#
# Перед запуском дерево должно быть чистым (`git status --short`) — скрипт не
# умеет различать «нашу» правку и чужую, поэтому работает только по текущему
# состоянию файла.

param(
    [Parameter(Mandatory)][string]$OldVersion,
    [Parameter(Mandatory)][string]$NewVersion
)

$ErrorActionPreference = 'Stop'

if ($OldVersion -eq $NewVersion) { throw "-OldVersion и -NewVersion совпадают." }

$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$files = @('Cargo.toml', 'Cargo.lock')
foreach ($d in @('crates', 'examples')) {
    if (Test-Path (Join-Path $root $d)) {
        $files += Get-ChildItem -Path (Join-Path $root $d) -Recurse -Filter 'Cargo.toml' -File | ForEach-Object { $_.FullName }
    }
}

# Пятнадцать workspace-крейтов (из корневой Cargo.lock) — для sanity-check.
$known = @(
    'light', 'light-demo', 'light-fw', 'light-fw-behavior-validation',
    'light-fw-cli-core', 'light-fw-core', 'light-fw-devtools-web',
    'light-fw-element-form', 'light-fw-html', 'light-fw-macros',
    'light-gui', 'light-reactivity', 'light-render-cpu', 'light-render-gpu',
    'light-vdom'
)

$total = 0
foreach ($file in $files) {
    $rel = [System.IO.Path]::GetRelativePath($root, $file)
    $lines = Get-Content -Path $file -Encoding utf8
    $changed = 0

    for ($i = 0; $i -lt $lines.Count; $i++) {
        $raw = $lines[$i]
        $trim = $raw.TrimStart()

        # Пропускаем комментарии (начинаются с '#') и строки description.
        if ($trim.StartsWith('#')) { continue }
        if ($trim -match '^description\s*=') { continue }

        $isVersionLine = $trim -match '^version\s*=\s*"([^"]+)"'
        $isPathDep     = $trim -match 'path\s*=\s*"[^"]+"\s*,\s*version\s*=\s*"([^"]+)"'

        if (-not ($isVersionLine -or $isPathDep)) { continue }

        $m = if ($isVersionLine) { [regex]::Match($raw, '(?<=version\s*=\s*\")[^"]+?(?="\s*$)') }
             else                 { [regex]::Match($raw, '(?<=version\s*=\s*\")[^"]+?(?="\s*\})') }
        if ($m.Success -and $m.Value -eq $OldVersion) {
            $lines[$i] = $raw -replace ([regex]::Escape($m.Value)), $NewVersion
            $changed++
        }
    }

    if ($changed -gt 0) {
        # Пишем UTF-8 без BOM (как в исходниках репозитория).
        [System.IO.File]::WriteAllLines($file, $lines, (New-Object System.Text.UTF8Encoding($false)))
        Write-Host ("{0,-44} {1} строк" -f $rel, $changed)
        $total += $changed
    }
}

if ($total -eq 0) {
    Write-Warning "Ничего не заменено: версия '$OldVersion' не найдена в функциональных полях."
    exit 1
}

# Sanity-check: все workspace-крейты теперь имеют новую версию в lock.
$lockLines = Get-Content (Join-Path $root 'Cargo.lock')
$missing = @()
for ($i = 0; $i -lt $lockLines.Count - 1; $i++) {
    if ($lockLines[$i] -match '^name = "([^"]+)"') {
        $pkg = $Matches[1]
        if ($known -contains $pkg -and $lockLines[$i+1] -ne "version = `"$NewVersion`"") {
            $missing += $pkg
        }
    }
}
if ($missing) {
    throw "Пакеты без новой версии в Cargo.lock: $($missing -join ', ')"
}

Write-Host ""
Write-Host ("Всего заменено: {0} (версия {1} -> {2})" -f $total, $OldVersion, $NewVersion)
Write-Host "Санити-чек: все $($known.Count) workspace-крейтов в Cargo.lock имеют $NewVersion. OK"
