param()

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$version = '5.42.2.1'
$sha256 = '32D83BE90CF04B807CFB9477482BC36302CDEE6F5B04CF57E81ADECBD8F07898'
$tools = Join-Path $root 'temp\tools'
$archive = Join-Path $tools "strawberry-perl-$version-64bit-portable.zip"
$destination = Join-Path $tools 'strawberry-portable'
$localPerl = Join-Path $destination 'perl\bin\perl.exe'

function Test-FullPerl([string]$Executable) {
    if (-not $Executable -or -not (Test-Path -LiteralPath $Executable -PathType Leaf)) {
        return $false
    }
    & $Executable -MLocale::Maketext::Simple -e 'exit 0' 2>$null
    return $LASTEXITCODE -eq 0
}

if (Test-FullPerl $localPerl) {
    Write-Output (Split-Path -Parent $localPerl)
    exit 0
}

$systemPerl = Get-Command perl -ErrorAction SilentlyContinue
if ($systemPerl -and (Test-FullPerl $systemPerl.Source)) {
    Write-Output (Split-Path -Parent $systemPerl.Source)
    exit 0
}

New-Item -ItemType Directory -Force -Path $tools | Out-Null
if (-not (Test-Path -LiteralPath $archive -PathType Leaf) -or
    (Get-FileHash -Algorithm SHA256 -LiteralPath $archive).Hash -ne $sha256) {
    $url = "https://github.com/StrawberryPerl/Perl-Dist-Strawberry/releases/download/SP_54221_64bit/strawberry-perl-$version-64bit-portable.zip"
    Write-Host "Downloading Strawberry Perl $version for the SQLCipher build..." -ForegroundColor Cyan
    & curl.exe -L --fail --retry 3 --output $archive $url
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}

$actualHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $archive).Hash
if ($actualHash -ne $sha256) {
    throw "Strawberry Perl SHA256 mismatch: $actualHash"
}

if (Test-Path -LiteralPath $destination) {
    Remove-Item -LiteralPath $destination -Recurse -Force
}
Expand-Archive -LiteralPath $archive -DestinationPath $destination
if (-not (Test-FullPerl $localPerl)) {
    throw 'The local Strawberry Perl archive was extracted but Perl is not usable.'
}
Write-Output (Split-Path -Parent $localPerl)
