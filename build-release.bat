@echo off
chcp 65001 >nul
echo ==============================================
echo   SpeakType - Release 版本建置（雙擊版）
echo ==============================================
echo.

powershell -ExecutionPolicy Bypass -File "%~dp0build.ps1" -Release

echo.
echo 按任意鍵結束...
pause >nul
