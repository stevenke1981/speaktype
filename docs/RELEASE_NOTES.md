SpeakType 0.5.4

## 更新內容
- **新增 Vulkan 版**：NVIDIA、AMD、Intel 顯示卡都能用 GPU 辨識，只需一般顯示卡驅動程式。
- **CUDA 版瘦身**：安裝程式不再內含 cuBLAS／CUDA 執行階段，改在安裝時從 NVIDIA 官方下載（約 430 MB，SHA256 驗證），失敗會自動改用本專案 GitHub 上的鏡像（同樣驗證 SHA256）；仍失敗則先用 CPU，之後可在設定中重新下載。
- **缺少 GPU 元件也能啟動**：GPU 版找不到驅動程式或 DLL 時自動改用 CPU，並在「設定 → 辨識硬體」顯示原因與目前使用的後端。
- 「辨識硬體」設定：自動／GPU (CUDA / Vulkan)／CPU。
- 延續 0.5.2：修正停在「轉寫中」、CPU 版約快 5 倍。

## 要下載哪個版本
| 你的電腦 | 下載 | 大小 |
|---|---|---|
| 不確定／沒有獨立顯示卡 | `speaktype-setup-windows-x64.exe`（CPU 版） | 約 13 MB |
| 有 AMD／Intel／NVIDIA 顯示卡 | `speaktype-vulkan-setup-windows-x64.exe`（Vulkan 版） | 約 17.7 MB |
| 有 NVIDIA 顯示卡、想要最快 | `speaktype-cuda-setup-windows-x64.exe`（CUDA 版，安裝時另下載約 430 MB） | 見檔案列表 |

每個版本都有免安裝 zip（`speaktype-*-windows-x64.zip`）。三種安裝程式裝在同一位置，可直接互相覆蓋。

**CUDA 版需求**：NVIDIA GTX 10 系列以上、驅動程式 R525 以上；CUDA 元件來源 `developer.download.nvidia.com/compute/cuda/redist`（cuda_cudart 12.6.77、libcublas 12.6.4.1），以 NVIDIA `redistrib_12.6.3.json` 公布的 SHA256 驗證。鏡像：[cuda-libs-12.6](https://github.com/stevenke1981/speaktype/releases/tag/cuda-libs-12.6)。
**Vulkan 版需求**：顯示卡驅動程式提供 Vulkan（`vulkan-1.dll`，一般顯示卡驅動都有）。

## 驗證
`Get-FileHash .\檔名 -Algorithm SHA256`，與 `SHA256SUMS*.txt` 比對。

## 尚未驗證
- CI 沒有 GPU：CUDA／Vulkan 的 GPU 辨識速度未實測，只驗證了沒有 GPU 時能啟動並以 CPU 辨識。
- 安裝時下載 CUDA 元件的流程未在實機上跑過。
