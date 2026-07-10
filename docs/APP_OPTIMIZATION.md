# SpeakType App 優化紀錄與建議

## 本次已完成

### 1. 原生 App 功能圖示

- 新增程式生成的 SpeakType 麥克風圖示。
- 圖示會顯示於視窗標題列、工作列與作業系統視窗切換介面。
- 不依賴外部 PNG/ICO，避免安裝包遺漏資源。
- 圖示沿用預設 Quiet Luxury 主題的暖棕色，保持品牌一致性。

### 2. 視窗與版面改善

- 預設視窗由 `560 × 560` 調整為 `640 × 680`，降低設定頁與結果區擁擠。
- 新增最小視窗尺寸 `480 × 520`，避免按鈕、工具列與文字區被壓縮到無法操作。
- 視窗標題改為「SpeakType · 語音輸入」，讓工作列辨識更直覺。

### 3. 正式版效能設定

- 啟用 Thin LTO。
- 將 codegen units 設為 1，提升跨模組最佳化效果。
- 移除正式版符號資訊，縮小執行檔。
- 正式版使用 `panic = abort`，避免桌面程式攜帶不必要的 unwind 成本。

### 4. 基本驗證

- 新增圖示尺寸、RGBA 緩衝區長度與透明度測試。
- 保留原本單一執行個體、系統匣、CUDA、模型與錄音流程。

## 下一階段建議

### P0：應優先處理

1. **統一工具列圖示系統**
   - 將目前分散於 `app.rs` 的 Emoji 字串集中到 `gui/icons.rs`。
   - 每個功能使用固定的圖示、標籤、tooltip 與快捷鍵說明。
   - 建議項目：歷史、錄音檔、模型、設定、錯誤、重新整理、隱藏。

2. **拆分 `src/app.rs`**
   - `app.rs` 已同時負責狀態、事件、設定頁、模型中心、錄音管理與主畫面。
   - 建議拆為：
     - `app/state.rs`
     - `app/events.rs`
     - `app/views/home.rs`
     - `app/views/settings.rs`
     - `app/views/models.rs`
     - `app/views/recordings.rs`
   - 可降低借用衝突、回歸風險與單檔維護成本。

3. **避免 UI 執行緒中的阻塞工作**
   - 模型下載、模型載入、音訊處理與檔案掃描應持續放在 worker。
   - 所有新功能需禁止在 egui `update()` 直接執行長時間 I/O。

### P1：使用體驗

1. 工具列寬度不足時改為自動換行或「更多」選單。
2. 增加鍵盤導覽與聚焦樣式，改善無滑鼠操作。
3. 最近辨識結果增加「複製、清除、重新送出」圖示按鈕。
4. 錯誤訊息增加「複製診斷資料」與「開啟日誌位置」。
5. 模型下載顯示剩餘時間、速度與檔案大小。
6. 錄音狀態加入更清楚的麥克風權限與輸入音量警示。

### P2：工程品質

1. 將 `whisper-rs` CUDA 功能改為 Cargo feature，提供 CPU-only 建置。
2. 為 Windows 建立 `.ico` 與安裝程式資源，補齊檔案總管中的正式圖示。
3. CI 增加：
   - `cargo fmt --check`
   - `cargo clippy --all-targets -- -D warnings`
   - `cargo test`
   - CPU-only smoke build
4. 加入設定檔 migration，避免未來欄位變更造成舊版設定載入失敗。
5. 對錄音保留、剪貼簿還原、文字注入與模型校驗補整合測試。

## 驗收建議

```powershell
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
```

Windows 手動驗收：

- 工作列與 Alt+Tab 顯示麥克風圖示。
- 視窗縮小至最小尺寸時，主要錄音按鈕仍可操作。
- 系統匣隱藏與還原正常。
- 錄音、轉錄、模型切換、設定、歷史與錯誤視窗皆可開啟。
- Release 版啟動與退出時沒有新增錯誤日誌。
