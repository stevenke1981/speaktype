param(
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"
$projectRoot = Split-Path -Parent $PSScriptRoot
$releaseExe = Join-Path $projectRoot "dist\release\speaktype.exe"
$packageRoot = Join-Path $projectRoot "dist\packages"
$staging = Join-Path $packageRoot "SpeakType-portable"
$zipPath = Join-Path $packageRoot "SpeakType-portable.zip"

Set-Location $projectRoot

if (-not $SkipBuild) {
    & (Join-Path $projectRoot "build.ps1") -Release
}

if (-not (Test-Path $releaseExe)) {
    throw "Release executable not found: $releaseExe"
}

if (Test-Path $staging) {
    Remove-Item -LiteralPath $staging -Recurse -Force
}
New-Item -ItemType Directory -Path $staging | Out-Null
New-Item -ItemType Directory -Path (Join-Path $staging "docs") | Out-Null

Copy-Item $releaseExe -Destination (Join-Path $staging "speaktype.exe") -Force
Copy-Item (Join-Path $projectRoot "PACKAGING.md") -Destination (Join-Path $staging "docs\PACKAGING.md") -Force

@'
SpeakType Portable

Run speaktype.exe to start the app.

User data is stored under:
%LOCALAPPDATA%\SpeakType\

Folders:
- config: app settings
- models: downloaded Whisper models
- recordings: saved WAV files
- logs: runtime logs
- diagnostics: exported debug bundles

Recordings and models are not included in this portable package.
'@ | Set-Content -Path (Join-Path $staging "README.txt") -Encoding ASCII

@'
param([switch]$Tray)
$exe = Join-Path $PSScriptRoot "speaktype.exe"
$value = "`"$exe`""
if ($Tray) { $value += " --tray" }
reg add "HKCU\Software\Microsoft\Windows\CurrentVersion\Run" /v SpeakType /t REG_SZ /d $value /f
'@ | Set-Content -Path (Join-Path $staging "install-startup.ps1") -Encoding ASCII

@'
reg delete "HKCU\Software\Microsoft\Windows\CurrentVersion\Run" /v SpeakType /f
'@ | Set-Content -Path (Join-Path $staging "uninstall-startup.ps1") -Encoding ASCII

if (Test-Path $zipPath) {
    Remove-Item -LiteralPath $zipPath -Force
}

Compress-Archive -Path (Join-Path $staging "*") -DestinationPath $zipPath -Force
Write-Host "Portable package created: $zipPath"
