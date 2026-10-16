# Silent per-user install / verify / uninstall of the Inno Setup installer.
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$setup = Join-Path $root 'dist\speaktype-setup-windows-x64.exe'
$dir = Join-Path $env:LOCALAPPDATA 'Programs\SpeakType'
$run = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
function Check($cond, $msg) { if (-not $cond) { throw "FAILED: $msg" } ; Write-Host "ok: $msg" }

$p = Start-Process $setup -Wait -PassThru -ArgumentList @(
    '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/CURRENTUSER', '/LANG=english',
    '/TASKS=autostart,desktopicon', '/LOG=install.log')
Check ($p.ExitCode -eq 0) "installer exit code 0 (got $($p.ExitCode))"
foreach ($f in 'speaktype.exe', 'README.md', 'README.en.md', 'unins000.exe') {
    Check (Test-Path (Join-Path $dir $f)) "installed $f"
}
Check (Test-Path (Join-Path ([Environment]::GetFolderPath('Programs')) 'SpeakType.lnk')) "Start menu shortcut"
Check (Test-Path (Join-Path ([Environment]::GetFolderPath('Desktop')) 'SpeakType.lnk')) "Desktop shortcut"
$auto = (Get-ItemProperty $run -ErrorAction SilentlyContinue).SpeakType
Check ($auto -like '*speaktype.exe*--tray*') "autostart Run value: $auto"

$ver = & (Join-Path $dir 'speaktype.exe') --version | Out-String
Check ($ver -like '*SpeakType*') "speaktype --version: $($ver.Trim())"

# Installed app starts and renders its first frame.
$probe = Join-Path $env:RUNNER_TEMP 'installed-startup.txt'
$env:SPEAKTYPE_SKIP_MODEL_LOAD = '1'
$p = Start-Process (Join-Path $dir 'speaktype.exe') -PassThru -ArgumentList '--startup-probe', "`"$probe`""
if (-not $p.WaitForExit(60000)) { Stop-Process -Id $p.Id -Force; throw "FAILED: installed app did not reach first frame in 60 s" }
Check (Test-Path $probe) "installed app first frame in $((Get-Content $probe).Trim()) ms"
Remove-Item env:SPEAKTYPE_SKIP_MODEL_LOAD

# Simulate user data so we can check it survives a silent uninstall without /PURGE.
$data = Join-Path $env:LOCALAPPDATA 'SpeakType'
New-Item -ItemType Directory -Force (Join-Path $data 'config') | Out-Null

$p = Start-Process (Join-Path $dir 'unins000.exe') -Wait -PassThru -ArgumentList '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART'
Check ($p.ExitCode -eq 0) "uninstaller exit code 0"
Start-Sleep -Seconds 3
Check (-not (Test-Path (Join-Path $dir 'speaktype.exe'))) "speaktype.exe removed"
Check (-not (Test-Path (Join-Path ([Environment]::GetFolderPath('Programs')) 'SpeakType.lnk'))) "Start menu shortcut removed"
Check (-not (Test-Path (Join-Path ([Environment]::GetFolderPath('Desktop')) 'SpeakType.lnk'))) "Desktop shortcut removed"
$auto = (Get-ItemProperty $run -ErrorAction SilentlyContinue).SpeakType
Check ($null -eq $auto) "autostart Run value removed"
Check (Test-Path $data) "user data kept on silent uninstall"
$uninstKey = Get-ChildItem 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall' | Where-Object { $_.PSChildName -like '*5E3B7F2A*' }
Check ($null -eq $uninstKey) "Add/Remove Programs entry removed"

# Reinstall + uninstall with /PURGE removes user data too.
$p = Start-Process $setup -Wait -PassThru -ArgumentList '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/CURRENTUSER', '/TASKS='
Check ($p.ExitCode -eq 0) "reinstall exit code 0"
Check ($null -eq (Get-ItemProperty $run -ErrorAction SilentlyContinue).SpeakType) "no Run value when autostart unchecked"
$p = Start-Process (Join-Path $dir 'unins000.exe') -Wait -PassThru -ArgumentList '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/PURGE'
Check ($p.ExitCode -eq 0) "purge uninstall exit code 0"
Start-Sleep -Seconds 3
Check (-not (Test-Path $data)) "user data removed with /PURGE"
Write-Host "Installer smoke test passed"
