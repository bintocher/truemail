param(
    [Parameter(Mandatory = $true)]
    [string]$Tag,
    [Parameter(Mandatory = $true)]
    [string]$Directory
)

# Заливает артефакты Windows-сборки в релиз. latest.json собирает отдельный job
# manifest (publish-manifest.sh) из артефактов всех платформ.
# ASCII-only: Windows PowerShell 5.1 читает .ps1 без BOM как ANSI, кириллица
# сломала бы парсинг на раннере.
$ErrorActionPreference = 'Stop'
$owner = 'chernov'
$repo = 'truemail'
$api = 'https://api.gitverse.ru'
$accept = 'application/vnd.gitverse.object+json;version=1'

if (-not $env:TOKEN) {
    throw 'TOKEN is not set: add the TRUEMAIL_GITVERSE_TOKEN secret'
}
if (-not (Test-Path -LiteralPath $Directory -PathType Container)) {
    throw "Package directory not found: $Directory"
}

$headers = @{
    Authorization = "Bearer $env:TOKEN"
    Accept = $accept
}

function Invoke-GitVerse([string]$Uri) {
    Invoke-RestMethod -Uri $Uri -Method Get -Headers $headers
}

function Get-Release {
    $releases = @(Invoke-GitVerse "$api/repos/$owner/$repo/releases")
    $release = $releases | Where-Object { $_.tag_name -eq $Tag } | Select-Object -First 1
    if ($release) { return $release }
    # Релиза ещё нет (linux/macos сборки идут параллельно) - создаём его.
    $body = @{
        tag_name = $Tag
        name = "truemail $Tag"
        body = "truemail $Tag"
        is_authorized_only = $false
    } | ConvertTo-Json -Compress
    Invoke-RestMethod -Uri "$api/repos/$owner/$repo/releases" -Method Post `
        -Headers $headers -ContentType 'application/json' -Body $body
}

function Get-Assets([object]$Release) {
    @(Invoke-GitVerse "$api/repos/$owner/$repo/releases/$($Release.id)/assets")
}

function Add-Asset([long]$ReleaseId, [System.IO.FileInfo]$File, [string[]]$ExistingNames) {
    if ($ExistingNames -contains $File.Name) {
        Write-Host "Asset $($File.Name) already in release"
        return
    }
    Write-Host "Uploading $($File.Name)"
    & curl.exe --silent --show-error --fail-with-body `
        -H "Authorization: Bearer $env:TOKEN" `
        -H "Accept: $accept" `
        -X POST "$api/repos/$owner/$repo/releases/$ReleaseId/assets" `
        -F "attachment=@$($File.FullName)" -F "name=$($File.Name)" | Out-Null
    if ($LASTEXITCODE -ne 0) {
        throw "Failed to upload $($File.Name)"
    }
}

$release = Get-Release
# GitVerse rejects the .sig extension (400) - upload signatures as .sig.txt.
Get-ChildItem -LiteralPath $Directory -Filter *.sig | ForEach-Object {
    Rename-Item -LiteralPath $_.FullName -NewName "$($_.Name).txt"
}
$existingNames = @((Get-Assets $release) | ForEach-Object { $_.name })
Get-ChildItem -LiteralPath $Directory -File | ForEach-Object {
    Add-Asset $release.id $_ $existingNames
}
Write-Host "Windows artifacts published to release $Tag"
