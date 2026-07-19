param(
    [Parameter(Mandatory = $true)]
    [string]$Tag,
    [Parameter(Mandatory = $true)]
    [string]$Directory
)

$ErrorActionPreference = 'Stop'
$owner = 'chernov'
$repo = 'truemail'
$api = 'https://api.gitverse.ru'
$accept = 'application/vnd.gitverse.object+json;version=1'

if (-not $env:TOKEN) {
    throw 'TOKEN не задан: добавьте секрет TRUEMAIL_GITVERSE_TOKEN'
}
if (-not (Test-Path -LiteralPath $Directory -PathType Container)) {
    throw "Каталог пакетов не найден: $Directory"
}

$headers = @{
    Authorization = "Bearer $env:TOKEN"
    Accept = $accept
}

function Invoke-GitVerse([string]$Uri, [string]$Method = 'Get', [object]$Body = $null) {
    $arguments = @{
        Uri = $Uri
        Method = $Method
        Headers = $headers
    }
    if ($null -ne $Body) {
        $arguments.ContentType = 'application/json'
        $arguments.Body = $Body | ConvertTo-Json -Depth 10 -Compress
    }
    Invoke-RestMethod @arguments
}

function Get-Release {
    $releases = @(Invoke-GitVerse "$api/repos/$owner/$repo/releases")
    $release = $releases | Where-Object { $_.tag_name -eq $Tag } | Select-Object -First 1
    if (-not $release) {
        throw "Релиз $Tag не найден: Linux job должен создать его первым"
    }
    $release
}

function Get-Assets([object]$Release) {
    @(Invoke-GitVerse "$api/repos/$owner/$repo/releases/$($Release.id)/assets")
}

function Add-Asset([long]$ReleaseId, [System.IO.FileInfo]$File, [string[]]$ExistingNames) {
    if ($ExistingNames -contains $File.Name) {
        Write-Host "Файл $($File.Name) уже есть в релизе"
        return
    }
    Write-Host "Загружаю $($File.Name)"
    & curl.exe --silent --show-error --fail-with-body `
        -H "Authorization: Bearer $env:TOKEN" `
        -H "Accept: $accept" `
        -X POST "$api/repos/$owner/$repo/releases/$ReleaseId/assets" `
        -F "attachment=@$($File.FullName)" -F "name=$($File.Name)" | Out-Null
    if ($LASTEXITCODE -ne 0) {
        throw "Не удалось загрузить $($File.Name)"
    }
}

function Read-AssetText([object]$Asset) {
    $response = Invoke-WebRequest -Uri $Asset.browser_download_url -Headers $headers -UseBasicParsing
    $response.Content.Trim()
}

function New-Platform([object[]]$Assets, [string]$PackagePattern) {
    $package = $Assets | Where-Object { $_.name -like $PackagePattern -and $_.name -notlike '*.sig' } |
        Select-Object -First 1
    if (-not $package) { return $null }
    # Подписи заливаются под .sig.txt (GitVerse отклоняет расширение .sig).
    $signature = $Assets | Where-Object { $_.name -eq "$($package.name).sig.txt" } | Select-Object -First 1
    if (-not $signature) {
        throw "Для $($package.name) не найдена подпись"
    }
    [ordered]@{
        signature = Read-AssetText $signature
        url = $package.browser_download_url
    }
}

$release = Get-Release
# GitVerse отклоняет расширение .sig (400) - заливаем подписи как .sig.txt.
Get-ChildItem -LiteralPath $Directory -Filter *.sig | ForEach-Object {
    Rename-Item -LiteralPath $_.FullName -NewName "$($_.Name).txt"
}
$existingAssets = Get-Assets $release
$existingNames = @($existingAssets | ForEach-Object { $_.name })
Get-ChildItem -LiteralPath $Directory -File | ForEach-Object {
    Add-Asset $release.id $_ $existingNames
}

$assets = Get-Assets $release
$windows = New-Platform $assets '*.exe'
if (-not $windows) { throw 'Windows updater package не найден в релизе' }
$platforms = [ordered]@{ 'windows-x86_64' = $windows }
$linux = New-Platform $assets '*.AppImage'
if ($linux) { $platforms['linux-x86_64'] = $linux }
# macOS Apple Silicon: updater берёт .app.tar.gz и его подпись. Ключ darwin-aarch64
# соответствует arm64-сборке из macos job.
$macos = New-Platform $assets '*.app.tar.gz'
if ($macos) { $platforms['darwin-aarch64'] = $macos }

$version = $Tag.TrimStart('v')
$manifest = [ordered]@{
    version = $version
    notes = "Обновление truemail $Tag"
    pub_date = [DateTime]::UtcNow.ToString('yyyy-MM-ddTHH:mm:ssZ')
    platforms = $platforms
}
$manifestJson = $manifest | ConvertTo-Json -Depth 10
$manifestPath = Join-Path $Directory 'latest.json'
[System.IO.File]::WriteAllText(
    [System.IO.Path]::GetFullPath($manifestPath),
    "$manifestJson`n",
    [System.Text.UTF8Encoding]::new($false)
)

$existingNames = @($assets | ForEach-Object { $_.name })
Add-Asset $release.id (Get-Item -LiteralPath $manifestPath) $existingNames

# GitVerse Pages уже публикует website/. Обновляем только манифест, а [skip ci]
# не запускает повторную сборку релиза из служебного коммита.
$contentUri = "$api/repos/$owner/$repo/contents/website/latest.json?ref=master"
$current = Invoke-GitVerse $contentUri
$encoded = [Convert]::ToBase64String([System.Text.Encoding]::UTF8.GetBytes("$manifestJson`n"))
Invoke-GitVerse "$api/repos/$owner/$repo/contents/website/latest.json" 'Put' ([ordered]@{
    branch = 'master'
    content = $encoded
    sha = $current.sha
    message = "chore: publish updater manifest for $Tag [skip ci]"
    signoff = $false
}) | Out-Null

Write-Host "Релиз $Tag дополнен, latest.json опубликован"
