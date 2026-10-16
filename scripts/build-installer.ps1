# Builds dist\speaktype-setup-windows-x64.exe with Inno Setup.
# Expects target\release\speaktype.exe to exist (cargo build --release).
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$iscc = Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 6\ISCC.exe'
if (-not (Test-Path $iscc)) {
    choco install innosetup -y --no-progress
    if ($LASTEXITCODE -ne 0) { throw "choco install innosetup failed" }
}
if (-not (Test-Path $iscc)) { throw "ISCC.exe not found at $iscc" }

$isl = 'installer\ChineseTraditional.isl'
if (-not (Test-Path $isl)) {
    $bundled = Join-Path (Split-Path $iscc) 'Languages\ChineseTraditional.isl'
    if (Test-Path $bundled) {
        Copy-Item $bundled $isl
    } else {
        Invoke-WebRequest -UseBasicParsing -OutFile $isl `
            'https://raw.githubusercontent.com/jrsoftware/issrc/is-6_4_3/Files/Languages/Unofficial/ChineseTraditional.isl'
    }
}

$m = Select-String -Path 'Cargo.toml' -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1
$version = $m.Matches[0].Groups[1].Value
Write-Host "Building installer for SpeakType $version"

New-Item -ItemType Directory -Force dist | Out-Null
& $iscc /Qp "/DAppVersion=$version" "/DSrcDir=$root\target\release" 'installer\speaktype.iss'
if ($LASTEXITCODE -ne 0) { throw "ISCC failed with exit code $LASTEXITCODE" }
Get-Item 'dist\speaktype-setup-windows-x64.exe' | Format-List Name, Length
