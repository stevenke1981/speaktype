# Changelog

## 0.5.1-test — 2026-10-16
### 修正
- 模型下載「存取被拒」(os error 5)：設定或程式旁的模型資料夾無法寫入時（例如安裝在 `C:\Program Files`），改下載到 `%LOCALAPPDATA%\SpeakType\models`。
- 已存在於舊位置（設定的資料夾、程式旁 `models\`、使用者資料夾）的模型會被找到並直接使用。
- 錯誤訊息更清楚：顯示無法寫入的路徑並提示權限／防毒原因；模型來源回應 HTTP 401/403 時顯示明確說明。
### 新增
- 單元測試：無法寫入的模型資料夾會退回使用者資料夾。
- 文件：重寫 README、新增使用說明（docs/USER_GUIDE.zh-TW.md / .en.md）與本更新紀錄。

## 0.5.0-test — 2026-10-16
### 變更
- 介面從 egui/eframe 改為 Slint；所有功能保留（主視窗、設定、紀錄、系統匣、快捷鍵、模型、錄音檔、錯誤）。
- Windows 使用 Skia 繪製（退回 Skia 軟體繪製 → Slint 軟體繪製），中文字型 JhengHei UI → YaHei UI → PMingLiU → SimSun。
- 事件驅動、閒置不重繪：Windows 閒置 CPU 單核 47.8% → 0%；Linux RSS 174 MB → 40–48 MB。
- 錄音指示器改為獨立透明、置頂、不搶焦點的視窗，修正後方深色方塊。
### 已知
- Windows `speaktype.exe` 因內建 Skia 由 19.1 MB 增加到 36.7 MB。

## 0.4.0-test
- CPU 辨識加速 2–3 倍、麥克風常駐預錄、量化預設模型、各 App 送出規則、Typeless 風格整理、AI 潤稿、Inno Setup 安裝程式。詳見 [docs/RELEASE_NOTES.md](docs/RELEASE_NOTES.md)。
