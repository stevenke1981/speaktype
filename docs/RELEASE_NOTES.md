SpeakType 0.5.3

## 更新內容
- 修正 CUDA 發布檢查：等待程式結束後才判讀退出碼，避免誤判無 NVIDIA 驅動時的啟動結果。
- 強化 CPU／CUDA 發布檔案一致性：安裝檔、免安裝 ZIP 與校驗碼必須成套；遇到缺件或內容衝突會停止發布，保留既有附件。
- **修正一直停在「轉寫中」**：背景轉錄程序發生錯誤時不再無聲結束，會顯示錯誤並回到待命；另加入逾時保護（90 秒＋錄音長度 × 20，最長 15 分鐘），逾時會提示改用較小模型或 GPU 版。
- **CPU 版辨識速度約快 5 倍**：Windows 版 whisper.cpp 改用 clang-cl 編譯（原為 MSVC），並固定 AVX2/FMA/F16C 指令集。CI 量測 ggml-base、11 秒音訊：5.5–6.9 秒 → 1.0–1.25 秒（與 Linux 相同）。CPU 上不再啟用 flash attention。
- **提供 NVIDIA CUDA 版**（`speaktype-cuda-*`）：包含 CUDA 執行階段；程式設計為沒有 NVIDIA 驅動時改用 CPU，實際推理驗證限制見下方。
- 介面與記錄檔顯示實際使用的後端（CPU／CUDA）；GPU 偵測優先選擇 NVIDIA 顯示卡，不再誤認虛擬顯示卡。

## 要下載哪個檔案
| 檔案 | 適用 |
|---|---|
| `speaktype-cuda-setup-windows-x64.exe` / `speaktype-cuda-windows-x64.zip` | 有 NVIDIA 顯示卡（GTX 10 系列以上），需 NVIDIA 驅動程式 R525 以上；檔案較大（含 CUDA 執行階段） |
| `speaktype-setup-windows-x64.exe` / `speaktype-windows-x64.zip` | 一般電腦（CPU） |
| `SHA256SUMS.txt`、`SHA256SUMS-cuda.txt` | 校驗碼 |

兩種安裝程式安裝至同一路徑，互換安裝尚未實機驗證。

## 安裝與驗證
1. 執行安裝程式（未簽章，SmartScreen 按「其他資訊 → 仍要執行」）。
2. 「設定 → 辨識硬體」：自動（預設，有 NVIDIA GPU 就用）／GPU (CUDA)／CPU。實際使用的後端顯示在模型狀態與記錄檔。
3. `Get-FileHash .\檔名 -Algorithm SHA256`，與 SHA256SUMS 比對。

## 尚未驗證
- 實體 NVIDIA GPU 推理與辨識速度：**NOT_RUN**（本輪驗證主機為 AMD 顯示卡）。
- 無 NVIDIA 驅動的 `--version` 啟動檢查不代表已完成 CPU fallback 推理測試。
- CUDA 安裝程式的安裝、升級與卸載，以及全新 Windows 環境的相容性，尚未完成實機驗證。
- AVX2 版在使用者電腦上的實際速度尚待確認。
