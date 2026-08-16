[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidatePattern('^v_\d+$')]
    [string]$ReleaseTag,

    [string]$PrivateKeyPath = 'D:\Agent\Smart-Spreadsheet-Secrets\smart-spreadsheet-updater.key',

    [string]$NotesPath,

    [string]$OutputDirectory,

    [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'

$repositoryRoot = Split-Path -Parent $PSScriptRoot
$tauriConfigPath = Join-Path $repositoryRoot 'src-tauri\tauri.conf.json'
$tauriConfig = Get-Content -LiteralPath $tauriConfigPath -Encoding UTF8 -Raw | ConvertFrom-Json
$appVersion = [string]$tauriConfig.version

if ($appVersion -notmatch '^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$') {
    throw "tauri.conf.json 中的版本号不是有效的 SemVer：$appVersion"
}
if (-not (Test-Path -LiteralPath $PrivateKeyPath -PathType Leaf)) {
    throw "找不到更新签名私钥：$PrivateKeyPath"
}
if ($NotesPath -and -not (Test-Path -LiteralPath $NotesPath -PathType Leaf)) {
    throw "找不到更新说明文件：$NotesPath"
}

if (-not $OutputDirectory) {
    $OutputDirectory = Join-Path $repositoryRoot "target\release\publish\$ReleaseTag"
}
$resolvedOutputParent = Split-Path -Parent $OutputDirectory
if (-not (Test-Path -LiteralPath $resolvedOutputParent)) {
    New-Item -ItemType Directory -Path $resolvedOutputParent -Force | Out-Null
}
if (Test-Path -LiteralPath $OutputDirectory) {
    $existingFiles = @(Get-ChildItem -LiteralPath $OutputDirectory -Force)
    if ($existingFiles.Count -gt 0) {
        throw "发布暂存目录不是空的，为避免覆盖已有产物已停止：$OutputDirectory"
    }
} else {
    New-Item -ItemType Directory -Path $OutputDirectory | Out-Null
}

if (-not $SkipBuild) {
    $previousPrivateKey = [Environment]::GetEnvironmentVariable('TAURI_SIGNING_PRIVATE_KEY', 'Process')
    try {
        [Environment]::SetEnvironmentVariable('TAURI_SIGNING_PRIVATE_KEY', $PrivateKeyPath, 'Process')
        Push-Location $repositoryRoot
        try {
            & npm.cmd run tauri build -- --bundles nsis --ci
            if ($LASTEXITCODE -ne 0) {
                throw "Tauri Release 构建失败，退出码：$LASTEXITCODE"
            }
        } finally {
            Pop-Location
        }
    } finally {
        [Environment]::SetEnvironmentVariable('TAURI_SIGNING_PRIVATE_KEY', $previousPrivateKey, 'Process')
    }
}

$nsisDirectory = Join-Path $repositoryRoot 'target\release\bundle\nsis'
$installer = Get-ChildItem -LiteralPath $nsisDirectory -Filter "*_${appVersion}_x64-setup.exe" -File |
    Sort-Object LastWriteTime -Descending |
    Select-Object -First 1
if (-not $installer) {
    throw "没有在 $nsisDirectory 找到版本 $appVersion 的 NSIS 安装器。"
}

$signaturePath = "$($installer.FullName).sig"
if (-not (Test-Path -LiteralPath $signaturePath -PathType Leaf)) {
    throw "安装器缺少更新签名：$signaturePath。请勿使用 -SkipBuild，或确认构建时已提供签名私钥。"
}

$assetName = "Smart-Spreadsheet_${appVersion}_x64-setup.exe"
$assetSignatureName = "$assetName.sig"
$stagedInstaller = Join-Path $OutputDirectory $assetName
$stagedSignature = Join-Path $OutputDirectory $assetSignatureName
Copy-Item -LiteralPath $installer.FullName -Destination $stagedInstaller
Copy-Item -LiteralPath $signaturePath -Destination $stagedSignature

$notes = if ($NotesPath) {
    Get-Content -LiteralPath $NotesPath -Encoding UTF8 -Raw
} else {
    "智能表格 $appVersion"
}
$signature = (Get-Content -LiteralPath $stagedSignature -Encoding UTF8 -Raw).Trim()
$downloadUrl = "https://github.com/Achilng/Smart-Spreadsheet/releases/download/$ReleaseTag/$assetName"
$manifest = [ordered]@{
    version = $appVersion
    notes = $notes.Trim()
    pub_date = (Get-Date).ToUniversalTime().ToString('o')
    platforms = [ordered]@{
        'windows-x86_64' = [ordered]@{
            signature = $signature
            url = $downloadUrl
        }
    }
}
$manifestPath = Join-Path $OutputDirectory 'latest.json'
$manifest | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $manifestPath -Encoding UTF8

$hash = (Get-FileHash -LiteralPath $stagedInstaller -Algorithm SHA256).Hash
Write-Host "更新发布产物已准备完成：$OutputDirectory"
Write-Host "版本：$appVersion  Release：$ReleaseTag"
Write-Host "安装器 SHA-256：$hash"
Write-Host '请把该目录中的安装器、.sig 和 latest.json 一起上传到同一个 GitHub Release。'
Get-ChildItem -LiteralPath $OutputDirectory -File | Select-Object Name, Length, LastWriteTime
