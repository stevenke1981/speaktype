# Launch the GUI in a few states and capture the screen so rendering (CJK text,
# settings, history, floating indicator) can be checked from CI artifacts.
param([string]$Exe = 'target\release\speaktype.exe', [string]$OutDir = 'win-shots')
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$env:SPEAKTYPE_SKIP_MODEL_LOAD = '1'
$log = "$env:LOCALAPPDATA\SpeakType\logs\speaktype.log"

function Capture([string]$name, [System.Drawing.Rectangle]$r) {
  $bmp = New-Object System.Drawing.Bitmap $r.Width, $r.Height
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.CopyFromScreen($r.Location, [System.Drawing.Point]::Empty, $r.Size)
  $bmp.Save((Join-Path $OutDir "$name.png"), [System.Drawing.Imaging.ImageFormat]::Png)
  $g.Dispose(); $bmp.Dispose()
}

function Shot([string]$name, [hashtable]$envs, [switch]$IndicatorCrop) {
  foreach ($k in @('SPEAKTYPE_UI_PAGE', 'SPEAKTYPE_DEMO_INDICATOR', 'SPEAKTYPE_DEMO_HISTORY')) { Remove-Item "env:$k" -ErrorAction SilentlyContinue }
  foreach ($k in $envs.Keys) { Set-Item "env:$k" $envs[$k] }
  $p = Start-Process $Exe -PassThru
  Start-Sleep -Seconds 8
  if ($p.HasExited) { Get-Content $log -ErrorAction SilentlyContinue | Select-Object -Last 20; throw "GUI exited early with code $($p.ExitCode)" }
  $b = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
  Capture $name $b
  if ($IndicatorCrop) {
    # Bottom-centre strip where the pill lives (logical 184x40, 88 px above the bottom edge).
    $w = 520; $h = 200
    Capture "$name-crop" (New-Object System.Drawing.Rectangle ([int](($b.Width - $w) / 2)), ($b.Height - $h), $w, $h)
  }
  Stop-Process -Id $p.Id -Force
  Start-Sleep -Milliseconds 800
  Write-Host "saved $name.png"
}

Shot 'win-main' @{}
Shot 'win-settings' @{ SPEAKTYPE_UI_PAGE = 'settings' }
Shot 'win-settings-speed' @{ SPEAKTYPE_UI_PAGE = 'speed' }
Shot 'win-history' @{ SPEAKTYPE_UI_PAGE = 'history'; SPEAKTYPE_DEMO_HISTORY = '1' }
Shot 'win-indicator' @{ SPEAKTYPE_DEMO_INDICATOR = '1' } -IndicatorCrop
Copy-Item (Join-Path $OutDir 'win-main.png') 'win-screenshot.png'

# Time to first frame (3 runs).
foreach ($k in @('SPEAKTYPE_UI_PAGE', 'SPEAKTYPE_DEMO_INDICATOR', 'SPEAKTYPE_DEMO_HISTORY')) { Remove-Item "env:$k" -ErrorAction SilentlyContinue }
foreach ($i in 1..3) {
  $probe = Join-Path $OutDir "startup-$i.txt"
  $p = Start-Process $Exe -PassThru -ArgumentList '--startup-probe', "`"$probe`""
  if (-not $p.WaitForExit(60000)) { Stop-Process -Id $p.Id -Force; throw "startup probe timed out" }
  Write-Host "startup run ${i}: $((Get-Content $probe).Trim()) ms to first frame"
}
Get-Content $log -ErrorAction SilentlyContinue | Select-String 'renderer' | Select-Object -Last 6 | ForEach-Object { $_.Line } | Tee-Object (Join-Path $OutDir 'renderer.txt')
