# =====================================================
# SpeakType - Windows 依賴安裝腳本
# 用途：安裝 LLVM + 設定 LIBCLANG_PATH（讓 whisper-rs CUDA 能編譯）
# =====================================================

Write-Host "=== SpeakType 依賴安裝腳本 ===" -ForegroundColor Cyan
Write-Host ""

# 1. 檢查是否已安裝 LLVM
$llvmPath = "C:\Program Files\LLVM"
$clangDll = Join-Path $llvmPath "bin\clang.dll"

if (Test-Path $clangDll) {
    Write-Host "[✓] 偵測到已安裝 LLVM" -ForegroundColor Green
    Write-Host "    路徑: $llvmPath" -ForegroundColor Gray
} else {
    Write-Host "[!] 未偵測到 LLVM，準備安裝..." -ForegroundColor Yellow
    
    # 嘗試使用 winget 安裝
    Write-Host "正在使用 winget 安裝 LLVM（可能需要幾分鐘）..." -ForegroundColor Cyan
    winget install -e --id LLVM.LLVM --accept-package-agreements --accept-source-agreements
    
    if ($LASTEXITCODE -ne 0) {
        Write-Host "[X] winget 安裝失敗，請手動下載安裝：" -ForegroundColor Red
        Write-Host "    https://github.com/llvm/llvm-project/releases/latest" -ForegroundColor Yellow
        Write-Host "    下載 LLVM-*-win64.exe 並安裝，記得勾選「Add LLVM to PATH」" -ForegroundColor Yellow
        exit 1
    }
    
    # 再次確認是否安裝成功
    if (-not (Test-Path $clangDll)) {
        Write-Host "[X] LLVM 安裝完成但找不到 clang.dll，請確認安裝路徑" -ForegroundColor Red
        exit 1
    }
    
    Write-Host "[✓] LLVM 安裝完成" -ForegroundColor Green
}

# 2. 設定 LIBCLANG_PATH 環境變數
Write-Host ""
Write-Host "正在設定 LIBCLANG_PATH 環境變數..." -ForegroundColor Cyan

$binPath = Join-Path $llvmPath "bin"

# 設定使用者層級環境變數（不需要管理員權限）
[System.Environment]::SetEnvironmentVariable("LIBCLANG_PATH", $binPath, "User")

# 立即更新目前 PowerShell session 的環境變數
$env:LIBCLANG_PATH = $binPath

Write-Host "[✓] LIBCLANG_PATH 已設定為: $binPath" -ForegroundColor Green

# 3. 驗證設定
Write-Host ""
Write-Host "=== 驗證結果 ===" -ForegroundColor Cyan
Write-Host "LIBCLANG_PATH = $env:LIBCLANG_PATH" -ForegroundColor White

if ($env:LIBCLANG_PATH -eq $binPath) {
    Write-Host "[✓] 環境變數設定成功！" -ForegroundColor Green
} else {
    Write-Host "[!] 環境變數可能需要重新開啟終端機才會生效" -ForegroundColor Yellow
}

# 4. 完成提示
Write-Host ""
Write-Host "=== 安裝完成 ===" -ForegroundColor Cyan
Write-Host "請執行以下步驟：" -ForegroundColor White
Write-Host "1. 完全關閉目前的終端機視窗" -ForegroundColor Yellow
Write-Host "2. 重新開啟終端機" -ForegroundColor Yellow
Write-Host "3. 執行以下指令測試：" -ForegroundColor Yellow
Write-Host ""
Write-Host "    cd C:\Users\steven\speaktype" -ForegroundColor Gray
Write-Host "    cargo check" -ForegroundColor Gray
Write-Host ""
Write-Host "如果 cargo check 成功，就代表環境已準備好！" -ForegroundColor Green
