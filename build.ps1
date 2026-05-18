# =====================================================
# SpeakType - Windows 建置腳本 (Debug / Release)
# 用法：
#   .\build.ps1                  # 建置 Debug 版本
#   .\build.ps1 -Release         # 建置 Release 版本
#   .\build.ps1 -Clean           # 清除後再建置
# =====================================================

param(
    [switch]$Release,
    [switch]$Clean,
    [switch]$Help
)

if ($Help) {
    Write-Host "SpeakType 建置腳本" -ForegroundColor Cyan
    Write-Host ""
    Write-Host "用法：" -ForegroundColor Yellow
    Write-Host "  .\build.ps1              # 建置 Debug 版本" -ForegroundColor White
    Write-Host "  .\build.ps1 -Release     # 建置 Release 版本（推薦發行）" -ForegroundColor White
    Write-Host "  .\build.ps1 -Clean       # 先清除再建置" -ForegroundColor White
    Write-Host "  .\build.ps1 -Release -Clean" -ForegroundColor White
    exit 0
}

$ErrorActionPreference = "Stop"
$projectRoot = $PSScriptRoot
Set-Location $projectRoot

$configuration = if ($Release) { "Release" } else { "Debug" }
$buildType = if ($Release) { "--release" } else { "" }

Write-Host "==============================================" -ForegroundColor Cyan
Write-Host "  SpeakType 建置腳本 ($configuration 版本)" -ForegroundColor Cyan
Write-Host "==============================================" -ForegroundColor Cyan
Write-Host ""

# 1. 清除舊建置（如果需要）
if ($Clean) {
    Write-Host "[1/6] 清除舊的建置快取..." -ForegroundColor Yellow
    cargo clean
    Write-Host "    已清除 target/ 目錄" -ForegroundColor Green
    Write-Host ""
}

# 2. 尋找並初始化 Visual Studio 2022
Write-Host "[2/6] 初始化 Visual Studio C++ 環境..." -ForegroundColor Yellow

$vsPath = $null
$vswhere = "C:\Program Files (x86)\Microsoft Visual Studio\Installer\vswhere.exe"
if (Test-Path $vswhere) {
    $vsPath = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
}

$vsCandidatePaths = @(
    "C:\Program Files\Microsoft Visual Studio\18\Community",
    "C:\Program Files\Microsoft Visual Studio\18\BuildTools",
    "C:\Program Files\Microsoft Visual Studio\18\Professional",
    "C:\Program Files\Microsoft Visual Studio\18\Enterprise",
    "C:\Program Files\Microsoft Visual Studio\2022\BuildTools",
    "C:\Program Files\Microsoft Visual Studio\2022\Community",
    "C:\Program Files\Microsoft Visual Studio\2022\Professional",
    "C:\Program Files\Microsoft Visual Studio\2022\Enterprise"
)

if (-not $vsPath) {
    foreach ($p in $vsCandidatePaths) {
        if (Test-Path $p) {
            $vsPath = $p
            break
        }
    }
}

if ($vsPath -is [array]) {
    $vsPath = $vsPath | Select-Object -First 1
}

if ($vsPath) {
    $vsPath = $vsPath.Trim()
}

foreach ($p in $vsCandidatePaths) {
    if (-not $vsPath -and (Test-Path $p)) {
        $vsPath = $p
        break
    }
}

if (-not $vsPath) {
    Write-Host "    [X] 未找到 Visual Studio C++ Build Tools！" -ForegroundColor Red
    Write-Host "    請安裝 Visual Studio Build Tools 並勾選 Desktop development with C++" -ForegroundColor Yellow
    exit 1
}

Write-Host "    找到 Visual Studio: $vsPath" -ForegroundColor Green

$vcvarsall = Join-Path $vsPath "VC\Auxiliary\Build\vcvarsall.bat"
if (Test-Path $vcvarsall) {
    Write-Host "    正在載入 x64 編譯環境..." -ForegroundColor Yellow
    
    # 使用暫存 .bat 檔案方式（最相容舊版 PowerShell，避開 && 問題）
    $tempBat = [System.IO.Path]::GetTempFileName() + ".bat"
    Set-Content -Path $tempBat -Encoding ASCII -Value @(
        "call `"$vcvarsall`" x64 >nul 2>&1",
        "set"
    )
    
    cmd /c $tempBat | ForEach-Object {
        if ($_ -match "^(.*?)=(.*)$") {
            Set-Item -Force -Path "env:$($matches[1])" -Value $matches[2]
        }
    }
    Remove-Item $tempBat -ErrorAction SilentlyContinue
    
    Write-Host "    [✓] Visual Studio 環境已載入" -ForegroundColor Green
} else {
    Write-Host "    [X] 找不到 vcvarsall.bat" -ForegroundColor Red
    exit 1
}
Write-Host ""

# 3. 確認必要工具
Write-Host "[3/6] 檢查編譯工具..." -ForegroundColor Yellow
$clPath = (Get-Command cl -ErrorAction SilentlyContinue).Source
if ($clPath) {
    Write-Host "    [✓] 找到 cl.exe" -ForegroundColor Green
} else {
    Write-Host "    [!] cl.exe 未在 PATH，請確認 Visual Studio 安裝" -ForegroundColor Yellow
}
Write-Host ""

# 4. 設定 LIBCLANG_PATH（whisper-rs 需要）
Write-Host "[4/6] 檢查 LIBCLANG_PATH..." -ForegroundColor Yellow
$llvmPath = "C:\Program Files\LLVM"
$clangDll = Join-Path $llvmPath "bin\clang.dll"

if (Test-Path $clangDll) {
    $env:LIBCLANG_PATH = Join-Path $llvmPath "bin"
    Write-Host "    [✓] LIBCLANG_PATH 已設定" -ForegroundColor Green
} else {
    Write-Host "    [!] 未找到 LLVM，建議先執行 install-deps.ps1" -ForegroundColor Yellow
}
Write-Host ""

# 4.5 CUDA 12.6 + newer MSVC may reject the host compiler during whisper.cpp build.
$env:CMAKE_CUDA_FLAGS = (($env:CMAKE_CUDA_FLAGS, "-allow-unsupported-compiler") -join " ").Trim()
Write-Host "[4.5/6] CUDA 編譯旗標: $env:CMAKE_CUDA_FLAGS" -ForegroundColor Yellow
Write-Host ""

# 5. 開始建置
Write-Host "[5/6] 開始建置 $configuration 版本..." -ForegroundColor Yellow
Write-Host "    指令: cargo build $buildType" -ForegroundColor Gray
Write-Host ""

$startTime = Get-Date

if ($Release) {
    cargo build --release
} else {
    cargo build
}

if ($LASTEXITCODE -ne 0) {
    Write-Host ""
    Write-Host "==============================================" -ForegroundColor Red
    Write-Host "  建置失敗！請查看上方錯誤訊息" -ForegroundColor Red
    Write-Host "==============================================" -ForegroundColor Red
    exit 1
}

$endTime = Get-Date
$duration = $endTime - $startTime
Write-Host ""
Write-Host "[6/6] 建置完成！" -ForegroundColor Green
Write-Host "    花費時間: $($duration.ToString('mm\:ss'))" -ForegroundColor Cyan
Write-Host ""

# 6. 顯示結果
$exeName = "speaktype.exe"
$outDir = Join-Path $projectRoot "dist"
$configOutDir = Join-Path $outDir $configuration.ToLowerInvariant()
if (-not (Test-Path $configOutDir)) {
    New-Item -ItemType Directory -Path $configOutDir | Out-Null
}

if ($Release) {
    $exePath = Join-Path $projectRoot "target\release\$exeName"
    Copy-Item $exePath -Destination (Join-Path $configOutDir $exeName) -Force
    Copy-Item $exePath -Destination (Join-Path $outDir $exeName) -Force
    
    $fileInfo = Get-Item $exePath
    Write-Host "=== Release 版本資訊 ===" -ForegroundColor Cyan
    Write-Host "執行檔位置: $exePath" -ForegroundColor White
    Write-Host "檔案大小: $([math]::Round($fileInfo.Length / 1MB, 2)) MB" -ForegroundColor White
    Write-Host "已複製到: $configOutDir\$exeName" -ForegroundColor Green
    Write-Host "Release 快捷副本: $outDir\$exeName" -ForegroundColor Green
} else {
    $exePath = Join-Path $projectRoot "target\debug\$exeName"
    Copy-Item $exePath -Destination (Join-Path $configOutDir $exeName) -Force

    Write-Host "=== Debug 版本資訊 ===" -ForegroundColor Cyan
    Write-Host "執行檔位置: $exePath" -ForegroundColor White
    Write-Host "已複製到: $configOutDir\$exeName" -ForegroundColor Green
    Write-Host "可直接執行: cargo run" -ForegroundColor Green
}

Write-Host ""
Write-Host "==============================================" -ForegroundColor Green
Write-Host "  SpeakType $configuration 版本建置成功！" -ForegroundColor Green
Write-Host "==============================================" -ForegroundColor Green

