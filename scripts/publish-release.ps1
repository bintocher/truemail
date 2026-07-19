param(
    [Parameter(Mandatory = $true)]
    [string]$Tag,
    [Parameter(Mandatory = $true)]
    [string]$Directory
)

# Заливает артефакты Windows-сборки в релиз. latest.json собирает отдельный job
# manifest (publish-manifest.sh) из артефактов всех платформ.
# Все запросы идут через curl.exe: PowerShell Invoke-RestMethod не отправляет
# обязательный GitVerse-заголовок Accept ...;version=1 как надо, из-за чего GET
# возвращает 404. ASCII-only (PS 5.1 читает .ps1 без BOM как ANSI).
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

function Api-Get([string]$Path) {
    $out = & curl.exe --silent --show-error --fail-with-body `
        -H "Authorization: Bearer $env:TOKEN" -H "Accept: $accept" "$api$Path"
    if ($LASTEXITCODE -ne 0) { throw "GET $Path failed: $out" }
    if (-not $out) { return $null }
    return $out | ConvertFrom-Json
}

function Get-ReleaseId {
    $releases = @(Api-Get "/repos/$owner/$repo/releases")
    $release = $releases | Where-Object { $_.tag_name -eq $Tag } | Select-Object -First 1
    if ($release) { return $release.id }
    # Релиза ещё нет (сборки идут параллельно) - создаём его.
    $body = "{`"tag_name`":`"$Tag`",`"name`":`"truemail $Tag`",`"body`":`"truemail $Tag`",`"is_authorized_only`":false}"
    $out = & curl.exe --silent --show-error --fail-with-body `
        -H "Authorization: Bearer $env:TOKEN" -H "Accept: $accept" `
        -H "Content-Type: application/json" -X POST -d $body "$api/repos/$owner/$repo/releases"
    if ($LASTEXITCODE -ne 0) {
        # Возможна гонка: другой job создал релиз между GET и POST. Перечитываем.
        $releases = @(Api-Get "/repos/$owner/$repo/releases")
        $release = $releases | Where-Object { $_.tag_name -eq $Tag } | Select-Object -First 1
        if ($release) { return $release.id }
        throw "Failed to create release: $out"
    }
    return ($out | ConvertFrom-Json).id
}

$releaseId = Get-ReleaseId

# GitVerse rejects the .sig extension (400) - upload signatures as .sig.txt.
Get-ChildItem -LiteralPath $Directory -Filter *.sig | ForEach-Object {
    Rename-Item -LiteralPath $_.FullName -NewName "$($_.Name).txt"
}

$existing = @(Api-Get "/repos/$owner/$repo/releases/$releaseId/assets") | ForEach-Object { $_.name }

Get-ChildItem -LiteralPath $Directory -File | ForEach-Object {
    if ($existing -contains $_.Name) {
        Write-Host "Asset $($_.Name) already in release"
        return
    }
    Write-Host "Uploading $($_.Name)"
    & curl.exe --silent --show-error --fail-with-body `
        -H "Authorization: Bearer $env:TOKEN" -H "Accept: $accept" `
        -X POST "$api/repos/$owner/$repo/releases/$releaseId/assets" `
        -F "attachment=@$($_.FullName)" -F "name=$($_.Name)" | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "Failed to upload $($_.Name)" }
}
Write-Host "Windows artifacts published to release $Tag"
