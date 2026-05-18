# =====================================================
# SpeakType - CUDA 專用建置腳本 (推薦使用 CUDA 12.1)
# 用法：
#   .\build-with-cuda.ps1              # Debug + CUDA
#   .\build-with-cuda.ps1 -Release     # Release + CUDA
#   .\build-with-cuda.ps1 -Clean       # 清除後再建置
# =====================================================

param(
    [switch]$Release,
    [switch]$Clean,
    [switch]$Help
)

if ($Help) {
    Write-Host "SpeakType CUDA 建置腳本" -ForegroundColor Cyan
    Write-Host ""
    Write-Host "用法：" -ForegroundColor Yellow
    Write-Host "  .\build-with-cuda.ps1              # Debug + CUDA 12.1" -ForegroundColor White
    Write-Host "  .\build-with-cuda.ps1 -Release     # Release + CUDA 12.1" -ForegroundColor White
    Write-Host "  .\build-with-cuda.ps1 -Clean       # 先清除 whisper-rs-sys 快取" -ForegroundColor White
    exit 0
}

$ErrorActionPreference = "Stop"
$projectRoot = $PSScriptRoot
Set-Location $projectRoot

$configuration = if ($Release) { "Release" } else { "Debug" }
$buildType = if ($Release) { "--release" } else { "" }

Write-Host "==============================================" -ForegroundColor Cyan
Write-Host "  SpeakType CUDA 建置 ($configuration) - CUDA 12.1" -ForegroundColor Cyan
Write-Host "==============================================" -ForegroundColor Cyan
Write-Host ""

# 1. 設定 CUDA 12.1 環境（優先使用 12.1）
Write-Host "[1/6] 設定 CUDA 12.1 環境..." -ForegroundColor Yellow
$cuda12Path = "C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v12.1"
if (Test-Path $cuda12Path) {
    $env:CUDA_PATH = $cuda12Path
    $env:CUDA_PATH_V12_1 = $cuda12Path
    $env:PATH = "$cuda12Path\bin;$env:PATH"
    Write-Host "    [✓] 已切換至 CUDA 12.1" -ForegroundColor Green
} else {
    Write-Host "    [!] 未找到 CUDA 12.1，嘗試使用系統預設 CUDA" -ForegroundColor Yellow
}

# CUDA 12.6 with newer Visual Studio toolsets rejects MSVC 18 by default.
$env:CMAKE_CUDA_FLAGS = (($env:CMAKE_CUDA_FLAGS, "-allow-unsupported-compiler") -join " ").Trim()
Write-Host "    [✓] CUDA 編譯旗標: $env:CMAKE_CUDA_FLAGS" -ForegroundColor Green

# 2. 強制使用 Ninja 生成器（解決 cmake 配置問題）
Write-Host "[2/6] 設定 Ninja 生成器..." -ForegroundColor Yellow
$ninjaPath = "C:\Program Files\CMake\bin\ninja.exe"
if (Test-Path $ninjaPath) {
    $env:CMAKE_GENERATOR = "Ninja"
    $env:CMAKE_MAKE_PROGRAM = $ninjaPath
    Write-Host "    [✓] 已啟用 Ninja 生成器" -ForegroundColor Green
} else {
    Write-Host "    [!] 建議先安裝 Ninja: winget install Ninja-build.Ninja" -ForegroundColor Yellow
}

# 3. 清除 whisper-rs-sys 建置快取（最重要！）
if ($Clean) {
    Write-Host "[3/6] 清除 whisper-rs-sys 快取..." -ForegroundColor Yellow
    Get-ChildItem -Path "target\debug\build" -Directory -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -like "whisper-rs-sys*" } |
        Remove-Item -Recurse -Force -ErrorAction SilentlyContinue
    cargo clean
    Write-Host "    [✓] whisper-rs-sys 快取已清除" -ForegroundColor Green
}

# 4. 載入 Visual Studio 2022 環境（重用原有邏輯）
Write-Host "[4/6] 初始化 Visual Studio C++ 環境..." -ForegroundColor Yellow
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
    "C:\Program Files\Microsoft Visual Studio\2022\Professional"
)

if (-not $vsPath) {
    foreach ($p in $vsCandidatePaths) {
        if (Test-Path $p) { $vsPath = $p; break }
    }
}

if ($vsPath -is [array]) {
    $vsPath = $vsPath | Select-Object -First 1
}

if ($vsPath) {
    $vsPath = $vsPath.Trim()
}

if ($vsPath) {
    $vcvarsall = Join-Path $vsPath "VC\Auxiliary\Build\vcvarsall.bat"
    if (Test-Path $vcvarsall) {
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
    }
} else {
    Write-Host "    [X] 未找到 Visual Studio C++ Build Tools" -ForegroundColor Red
    exit 1
}

# 5. 開始 CUDA 建置
Write-Host "[5/6] 開始 CUDA 建置 ($configuration)..." -ForegroundColor Yellow
Write-Host "    這可能需要 5~15 分鐘（第一次編譯 whisper.cpp）" -ForegroundColor Gray
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
    Write-Host "  CUDA 建置失敗！請檢查上方錯誤" -ForegroundColor Red
    Write-Host "==============================================" -ForegroundColor Red
    exit 1
}

$endTime = Get-Date
$duration = $endTime - $startTime
Write-Host ""
Write-Host "[6/6] CUDA 建置成功！" -ForegroundColor Green
Write-Host "    花費時間: $($duration.ToString('mm\:ss'))" -ForegroundColor Cyan
Write-Host ""
Write-Host "==============================================" -ForegroundColor Green
Write-Host "  SpeakType CUDA 版本建置完成！" -ForegroundColor Green
Write-Host "==============================================" -ForegroundColor Green

