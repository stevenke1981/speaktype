param(
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"
$projectRoot = Split-Path -Parent $PSScriptRoot
$nsi = Join-Path $projectRoot "installer\SpeakType.nsi"
$makensis = Get-Command makensis.exe -ErrorAction SilentlyContinue

Set-Location $projectRoot

& (Join-Path $PSScriptRoot "package-portable.ps1") -SkipBuild:$SkipBuild

if (-not $makensis) {
    Write-Warning "makensis.exe was not found. Install NSIS to build the installer."
    Write-Host "Portable ZIP is still available under dist\packages."
    exit 0
}

& $makensis.Source "/DPROJECT_ROOT=$projectRoot" $nsi
if ($LASTEXITCODE -ne 0) {
    throw "NSIS installer build failed."
}
