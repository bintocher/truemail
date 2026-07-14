[CmdletBinding(PositionalBinding = $false)]
param(
    [string]$WorkingDirectory,
    [Parameter(Mandatory = $true, ValueFromRemainingArguments = $true)]
    [string[]]$Command
)

$ErrorActionPreference = 'Stop'
$perlBin = & (Join-Path $PSScriptRoot 'ensure-perl.ps1')
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
$env:PATH = "$perlBin;$env:PATH"
if ($WorkingDirectory) {
    Set-Location -LiteralPath (Join-Path (Split-Path -Parent $PSScriptRoot) $WorkingDirectory)
}

& $Command[0] $Command[1..($Command.Count - 1)]
exit $LASTEXITCODE
