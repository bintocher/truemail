$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$perlBin = & (Join-Path $PSScriptRoot 'ensure-perl.ps1')
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
$env:PATH = "$perlBin;$env:PATH"
Set-Location -LiteralPath $root
& cargo clippy --workspace --all-targets --all-features -- -D warnings
exit $LASTEXITCODE
