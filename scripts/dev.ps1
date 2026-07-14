param(
    [switch]$Check
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$tauriDir = Join-Path $root 'apps\desktop\src-tauri'

if (-not (Test-Path -LiteralPath $tauriDir -PathType Container)) {
    throw "Tauri directory not found: $tauriDir"
}

Set-Location -LiteralPath $root

# Vendored OpenSSL (для SQLCipher) требует полный Perl только во время сборки.
# Урезанный Perl из Git for Windows не подходит. Скрипт возьмёт системный
# Strawberry Perl или один раз скачает проверенную portable-сборку в gitignored temp/.
$perlBin = & (Join-Path $PSScriptRoot 'ensure-perl.ps1')
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
$env:PATH = "$perlBin;$env:PATH"

# Локальные переопределения для разработки. Значения не печатаются в консоль.
$dotenv = Join-Path $root '.env'
if (Test-Path -LiteralPath $dotenv -PathType Leaf) {
    foreach ($line in Get-Content -LiteralPath $dotenv) {
        $trimmed = $line.Trim()
        if (-not $trimmed -or $trimmed.StartsWith('#')) {
            continue
        }

        $parts = $trimmed -split '=', 2
        if ($parts.Count -ne 2) {
            continue
        }

        $name = $parts[0].Trim()
        $value = $parts[1].Trim().Trim('"').Trim("'")
        if ($name -match '^[A-Za-z_][A-Za-z0-9_]*$') {
            [Environment]::SetEnvironmentVariable($name, $value, 'Process')
        }
    }
}

if ($Check) {
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        throw 'cargo not found in PATH'
    }
    Write-Host 'truemail dev environment: OK'
    exit 0
}

if (-not (Get-Command cargo-sweep -ErrorAction SilentlyContinue)) {
    & cargo install cargo-sweep --version '0.8.0' --locked
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }
}

$exitCode = 0
try {
    Set-Location -LiteralPath $tauriDir
    & cargo tauri dev
    $exitCode = $LASTEXITCODE
}
finally {
    Set-Location -LiteralPath $root
    & cargo sweep --time 30 .
    if ($LASTEXITCODE -ne 0 -and $exitCode -eq 0) {
        $exitCode = $LASTEXITCODE
    }
}

exit $exitCode
