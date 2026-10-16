# Measures idle CPU of the GUI (main window visible, no model loaded, tray active).
param([string]$Exe = "target\release\speaktype.exe", [int]$Seconds = 30, [string]$Out = "win-shots\idle-cpu.txt")
$ErrorActionPreference = 'Stop'
New-Item -ItemType Directory -Force (Split-Path $Out) | Out-Null
$env:SPEAKTYPE_SKIP_MODEL_LOAD = '1'
$p = Start-Process $Exe -PassThru
Start-Sleep 8
if ($p.HasExited) { throw "GUI exited early with code $($p.ExitCode)" }
$p.Refresh(); $c0 = $p.TotalProcessorTime.TotalMilliseconds; $t0 = Get-Date
Start-Sleep $Seconds
$p.Refresh(); $c1 = $p.TotalProcessorTime.TotalMilliseconds; $t1 = Get-Date
$ws = [math]::Round($p.WorkingSet64 / 1MB)
Stop-Process -Id $p.Id -Force
Remove-Item env:SPEAKTYPE_SKIP_MODEL_LOAD
$wall = ($t1 - $t0).TotalMilliseconds
$pct = [math]::Round(100 * ($c1 - $c0) / $wall, 2)
$cores = [Environment]::ProcessorCount
$line = "idle CPU: $pct% of one core ($([math]::Round($pct / $cores, 2))% of $cores cores) over $([math]::Round($wall/1000))s, CPU time $([math]::Round($c1-$c0)) ms, working set $ws MB"
Write-Host $line
Set-Content $Out $line
