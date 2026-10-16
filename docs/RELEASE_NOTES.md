SpeakType 0.5.2-test（測試版）

## 更新內容
- **修正一直停在「轉寫中」**：背景轉錄程序發生錯誤時不再無聲結束，會顯示錯誤並回到待命；另加入逾時保護（90 秒＋錄音長度 × 20，最長 15 分鐘），逾時會提示改用較小模型或 GPU 版。
- **CPU 版加速設定**：Windows 版 whisper.cpp 改以固定的 AVX2/FMA/F16C 指令集編譯（不再依 CI 機器的 CPU「native」編譯）；CPU 上不再啟用 flash attention。
- **新增 NVIDIA CUDA 版**（`speaktype-cuda-*`）：使用 GPU 辨識；電腦沒有 NVIDIA 驅動程式時仍可啟動並自動改用 CPU。
- 介面與記錄檔顯示實際使用的後端（CPU／CUDA）；GPU 偵測優先選擇 NVIDIA 顯示卡，不再誤認虛擬顯示卡。

## 要下載哪個檔案
| 檔案 | 適用 |
|---|---|
| `speaktype-cuda-setup-windows-x64.exe` / `speaktype-cuda-windows-x64.zip` | 有 NVIDIA 顯示卡（GTX 10 系列以上），需 NVIDIA 驅動程式 R525 以上；檔案較大（含 CUDA 執行階段） |
| `speaktype-setup-windows-x64.exe` / `speaktype-windows-x64.zip` | 一般電腦（CPU） |
| `SHA256SUMS.txt`、`SHA256SUMS-cuda.txt` | 校驗碼 |

兩種安裝程式安裝到同一個位置，可直接互相覆蓋安裝。

## 安裝與驗證
1. 執行安裝程式（未簽章，SmartScreen 按「其他資訊 → 仍要執行」）。
2. 設定中「啟用 CUDA 推論」保持開啟即會在可用時使用 GPU。
3. `Get-FileHash .\檔名 -Algorithm SHA256`，與 SHA256SUMS 比對。

## 尚未驗證
- 尚未在實體 NVIDIA 顯示卡上實測 CUDA 版辨識速度。
- AVX2 版在使用者電腦上的實際速度尚待確認。
