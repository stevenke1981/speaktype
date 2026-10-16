# Builds dist\speaktype-windows-x64.zip (portable, no installer).
param([switch]$SkipBuild, [string]$Name = 'speaktype-windows-x64')
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
if (-not $SkipBuild) {
    cargo build --release --locked
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
}
$name = $Name
$stage = Join-Path 'dist' $name
Remove-Item -Recurse -Force $stage -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $stage | Out-Null
Copy-Item 'target\release\speaktype.exe', 'README.md', 'README.en.md' $stage
Get-ChildItem 'target\release\*.dll' -ErrorAction SilentlyContinue | Copy-Item -Destination $stage
Compress-Archive -Force -Path $stage -DestinationPath "dist\$name.zip"
Get-Item "dist\$name.zip" | Format-List Name, Length
