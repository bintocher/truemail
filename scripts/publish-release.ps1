param(
    [Parameter(Mandatory = $true)]
    [string]$Tag,
    [Parameter(Mandatory = $true)]
    [string]$Directory
)

# ASCII-only: Windows PowerShell 5.1 reads .ps1 without BOM as ANSI, so any
# Cyrillic here would corrupt parsing on the runner. Keep messages in English.
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
        throw "Release $Tag not found: the Linux job must create it first"
    }
    $release
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

function Read-AssetText([object]$Asset) {
    $response = Invoke-WebRequest -Uri $Asset.browser_download_url -Headers $headers -UseBasicParsing
    $response.Content.Trim()
}

function New-Platform([object[]]$Assets, [string]$PackagePattern) {
    $package = $Assets | Where-Object { $_.name -like $PackagePattern -and $_.name -notlike '*.sig*' } |
        Select-Object -First 1
    if (-not $package) { return $null }
    # Signatures are uploaded as .sig.txt (GitVerse rejects the .sig extension).
    $signature = $Assets | Where-Object { $_.name -eq "$($package.name).sig.txt" } | Select-Object -First 1
    if (-not $signature) {
        throw "Signature not found for $($package.name)"
    }
    [ordered]@{
        signature = Read-AssetText $signature
        url = $package.browser_download_url
    }
}

$release = Get-Release
# GitVerse rejects the .sig extension (400) - upload signatures as .sig.txt.
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
if (-not $windows) { throw 'Windows updater package not found in release' }
$platforms = [ordered]@{ 'windows-x86_64' = $windows }
$linux = New-Platform $assets '*.AppImage'
if ($linux) { $platforms['linux-x86_64'] = $linux }
# macOS Apple Silicon: updater uses .app.tar.gz and its signature. The key
# darwin-aarch64 matches the arm64 build from the macos job.
$macos = New-Platform $assets '*.app.tar.gz'
if ($macos) { $platforms['darwin-aarch64'] = $macos }

$version = $Tag.TrimStart('v')
$manifest = [ordered]@{
    version = $version
    notes = "truemail update $Tag"
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

# GitVerse Pages already serves website/. Update only the manifest; [skip ci]
# avoids triggering another release build from this service commit.
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

Write-Host "Release $Tag updated, latest.json published"
