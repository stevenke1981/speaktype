# Changelog

## 0.5.4 — 2026-10-17
### 新增
- Vulkan 版（`speaktype-vulkan-*`），支援 NVIDIA／AMD／Intel GPU；沒有 Vulkan 驅動時改用 CPU。
- CUDA 版改在安裝時從 NVIDIA 官方 redist 下載 cudart／cuBLAS（SHA256 驗證，3 次重試，失敗改用 GitHub 鏡像 `cuda-libs-12.6`），安裝程式不再內含這些 DLL。
- 程式偵測缺少的 CUDA DLL，在「設定 → 辨識硬體」顯示原因並提供「下載 CUDA 元件」按鈕。
- GPU 執行階段 DLL 改為延遲載入，缺少時以 stub 回報「無裝置」，程式不會當掉而改用 CPU。
### 變更
- CUDA 版的 C/C++ 部分也改用 clang-cl 編譯（CPU 後備路徑同樣加速）。
- 「辨識硬體」選項：自動／GPU (CUDA / Vulkan)／CPU。

## 0.5.3 — 2026-10-16
### 修正
- CUDA 發布檢查明確等待 GUI 程式結束，再讀取退出碼，避免將尚未取得的退出碼誤判為缺少 NVIDIA 驅動。
- CPU／CUDA 發布前分組驗證安裝檔、免安裝 ZIP 與校驗碼；既有檔案完全一致才略過，缺件或內容衝突即停止，保留舊附件。
- 同一 tag 的發布依序執行，避免重跑互相覆寫；tag 必須與 Cargo 版本一致。
### 驗證限制
- CI 驗證編譯、封裝與無 NVIDIA 驅動時的版本啟動；實體 NVIDIA GPU 推理、辨識速度及 CPU fallback 推理尚未完成硬體驗證。

## 0.5.2-test — 2026-10-16
### 修正
- 轉錄停在「轉寫中」：背景轉錄程序 panic 時會回報錯誤並重設；介面加入逾時保護。
- Windows CPU 版辨識約快 5 倍：whisper.cpp 改用 clang-cl 編譯並固定 AVX2/FMA/F16C（CI：ggml-base 11 秒音訊 5.5–6.9 秒 → 1.0–1.25 秒）；CPU 上不再啟用 flash attention。
- 新增「辨識硬體」設定：自動／GPU (CUDA)／CPU。
- GPU 偵測優先選 NVIDIA，不再顯示虛擬顯示卡；模型狀態顯示實際後端，不再在 CPU 版顯示「CUDA」。
### 新增
- NVIDIA CUDA 版安裝程式／免安裝版（nvcuda.dll 延遲載入，沒有 NVIDIA 驅動時改用 CPU）。
- 記錄檔記下 whisper 後端與 CPU 指令集（system_info）。

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
