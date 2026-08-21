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

function Set-TauriNsisBundleMarker {
    param([Parameter(Mandatory = $true)][string]$BinaryPath)

    $unknownToken = '__TAURI_BUNDLE_TYPE_VAR_UNK'
    $nsisToken = '__TAURI_BUNDLE_TYPE_VAR_NSS'
    $stream = [System.IO.File]::Open(
        $BinaryPath,
        [System.IO.FileMode]::Open,
        [System.IO.FileAccess]::ReadWrite,
        [System.IO.FileShare]::None
    )
    try {
        $bytes = [byte[]]::new($stream.Length)
        $offset = 0
        while ($offset -lt $bytes.Length) {
            $read = $stream.Read($bytes, $offset, $bytes.Length - $offset)
            if ($read -eq 0) {
                throw "读取主程序时提前结束：$BinaryPath"
            }
            $offset += $read
        }

        $binaryText = [System.Text.Encoding]::ASCII.GetString($bytes)
        $markerIndex = $binaryText.IndexOf($unknownToken, [System.StringComparison]::Ordinal)
        if ($markerIndex -lt 0) {
            throw "主程序中没有找到待写入的 Tauri 安装包类型标记：$BinaryPath"
        }
        if ($binaryText.IndexOf($unknownToken, $markerIndex + 1, [System.StringComparison]::Ordinal) -ge 0) {
            throw "主程序中出现多个 Tauri 安装包类型标记，已停止以避免写错位置：$BinaryPath"
        }

        $replacement = [System.Text.Encoding]::ASCII.GetBytes($nsisToken)
        [void]$stream.Seek($markerIndex, [System.IO.SeekOrigin]::Begin)
        $stream.Write($replacement, 0, $replacement.Length)
        $stream.Flush($true)
    } finally {
        $stream.Dispose()
    }
}

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
            # 将编译与打包拆开，便于在 Windows 上单独校正 NSIS 类型标记。
            & npm.cmd run tauri build -- --no-bundle --ci
            if ($LASTEXITCODE -ne 0) {
                throw "Tauri Release 构建失败，退出码：$LASTEXITCODE"
            }

            $mainBinaryPath = Join-Path $repositoryRoot 'target\release\smart-spreadsheet.exe'
            $exclusiveAccessReady = $false
            for ($attempt = 1; $attempt -le 30; $attempt++) {
                try {
                    $stream = [System.IO.File]::Open(
                        $mainBinaryPath,
                        [System.IO.FileMode]::Open,
                        [System.IO.FileAccess]::ReadWrite,
                        [System.IO.FileShare]::None
                    )
                    $stream.Dispose()
                    $exclusiveAccessReady = $true
                    break
                } catch {
                    Start-Sleep -Milliseconds 500
                }
            }
            if (-not $exclusiveAccessReady) {
                throw "主程序持续被占用，无法安全写入安装包类型标记：$mainBinaryPath"
            }

            Start-Sleep -Seconds 2
            & npm.cmd run tauri bundle -- --bundles nsis --ci
            if ($LASTEXITCODE -ne 0) {
                throw "Tauri NSIS 打包失败，退出码：$LASTEXITCODE"
            }

            # Tauri CLI 2.11.2 在部分 Windows 环境中会先持有主程序的只读句柄，
            # 随后回写类型标记时触发 os error 32。保留 Tauri 生成的完整 NSIS 脚本，
            # 在独占句柄内写入同一官方标记后重新运行 makensis，并重新生成更新签名。
            $nsisWorkDirectory = Join-Path $repositoryRoot 'target\release\nsis\x64'
            $nsisScriptPath = Join-Path $nsisWorkDirectory 'installer.nsi'
            $makeNsisPath = Join-Path $env:LOCALAPPDATA 'tauri\NSIS\makensis.exe'
            if (-not (Test-Path -LiteralPath $nsisScriptPath -PathType Leaf)) {
                throw "没有找到 Tauri 生成的 NSIS 脚本：$nsisScriptPath"
            }
            if (-not (Test-Path -LiteralPath $makeNsisPath -PathType Leaf)) {
                throw "没有找到 Tauri 使用的 makensis：$makeNsisPath"
            }

            $mainBinaryBackupPath = "$mainBinaryPath.bundle-original-$PID"
            Copy-Item -LiteralPath $mainBinaryPath -Destination $mainBinaryBackupPath
            try {
                Set-TauriNsisBundleMarker -BinaryPath $mainBinaryPath
                Push-Location $nsisWorkDirectory
                try {
                    & $makeNsisPath 'installer.nsi'
                    if ($LASTEXITCODE -ne 0) {
                        throw "校正类型标记后的 NSIS 打包失败，退出码：$LASTEXITCODE"
                    }
                } finally {
                    Pop-Location
                }

                $rebuiltInstallerPath = Join-Path $nsisWorkDirectory 'nsis-output.exe'
                $bundleDirectory = Join-Path $repositoryRoot 'target\release\bundle\nsis'
                $bundleInstaller = Get-ChildItem -LiteralPath $bundleDirectory -Filter "*_${appVersion}_x64-setup.exe" -File |
                    Sort-Object LastWriteTime -Descending |
                    Select-Object -First 1
                if (-not $bundleInstaller) {
                    throw "没有找到 Tauri 生成的 NSIS 安装器，无法替换为校正后的产物。"
                }
                if (-not (Test-Path -LiteralPath $rebuiltInstallerPath -PathType Leaf)) {
                    throw "makensis 没有生成预期安装器：$rebuiltInstallerPath"
                }

                Move-Item -LiteralPath $rebuiltInstallerPath -Destination $bundleInstaller.FullName -Force
                $rebuiltSignaturePath = "$($bundleInstaller.FullName).sig"
                if (Test-Path -LiteralPath $rebuiltSignaturePath) {
                    Remove-Item -LiteralPath $rebuiltSignaturePath -Force
                }
                $signerPrivateKey = [Environment]::GetEnvironmentVariable('TAURI_SIGNING_PRIVATE_KEY', 'Process')
                try {
                    [Environment]::SetEnvironmentVariable('TAURI_SIGNING_PRIVATE_KEY', $null, 'Process')
                    & npm.cmd run tauri -- signer sign --private-key-path $PrivateKeyPath '--password=' $bundleInstaller.FullName
                    if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath $rebuiltSignaturePath -PathType Leaf)) {
                        throw "校正后的 NSIS 安装器签名失败。"
                    }
                } finally {
                    [Environment]::SetEnvironmentVariable('TAURI_SIGNING_PRIVATE_KEY', $signerPrivateKey, 'Process')
                }
            } finally {
                Move-Item -LiteralPath $mainBinaryBackupPath -Destination $mainBinaryPath -Force
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
