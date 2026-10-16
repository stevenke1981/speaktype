# Launch the GUI in a few states and capture the screen so rendering (CJK text,
# settings, floating indicator) can be checked from CI artifacts.
param([string]$Exe = 'target\release\speaktype.exe', [string]$OutDir = 'win-shots')
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$env:SPEAKTYPE_SKIP_MODEL_LOAD = '1'

function Shot([string]$name, [hashtable]$envs) {
  foreach ($k in @('SPEAKTYPE_UI_PAGE', 'SPEAKTYPE_DEMO_INDICATOR')) { Remove-Item "env:$k" -ErrorAction SilentlyContinue }
  foreach ($k in $envs.Keys) { Set-Item "env:$k" $envs[$k] }
  $p = Start-Process $Exe -PassThru
  Start-Sleep -Seconds 8
  if ($p.HasExited) { throw "GUI exited early with code $($p.ExitCode)" }
  $b = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
  $bmp = New-Object System.Drawing.Bitmap $b.Width, $b.Height
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.CopyFromScreen($b.Location, [System.Drawing.Point]::Empty, $b.Size)
  $bmp.Save((Join-Path $OutDir "$name.png"), [System.Drawing.Imaging.ImageFormat]::Png)
  Stop-Process -Id $p.Id -Force
  Start-Sleep -Milliseconds 800
  Write-Host "saved $name.png"
}

Shot 'win-main' @{}
Shot 'win-settings-speed' @{ SPEAKTYPE_UI_PAGE = 'speed' }
Shot 'win-history' @{ SPEAKTYPE_UI_PAGE = 'history' }
Shot 'win-indicator' @{ SPEAKTYPE_DEMO_INDICATOR = '1' }
Copy-Item (Join-Path $OutDir 'win-main.png') 'win-screenshot.png'

# Time to first frame (3 runs).
foreach ($i in 1..3) {
  $probe = Join-Path $OutDir "startup-$i.txt"
  $p = Start-Process $Exe -PassThru -ArgumentList '--startup-probe', "`"$probe`""
  if (-not $p.WaitForExit(60000)) { Stop-Process -Id $p.Id -Force; throw "startup probe timed out" }
  Write-Host "startup run ${i}: $((Get-Content $probe).Trim()) ms to first frame"
}
