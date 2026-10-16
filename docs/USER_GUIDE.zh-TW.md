# SpeakType 使用說明（v0.5.4）

[English](USER_GUIDE.en.md) ｜ [回到 README](../README.md)

## 1. 安裝

1. 從 [Releases](https://github.com/stevenke1981/speaktype/releases) 下載 `speaktype-setup-windows-x64.exe`。
2. （建議）用 `SHA256SUMS.txt` 驗證：
   ```powershell
   Get-FileHash .\speaktype-setup-windows-x64.exe -Algorithm SHA256
   ```
   比對結果與 `SHA256SUMS.txt` 中同名檔案的值。
3. 執行安裝程式。可選「只為我安裝」（不需系統管理員）或「所有使用者」（安裝到 `C:\Program Files\SpeakType`）。可勾選「登入 Windows 時自動啟動」。
4. 免安裝版：解壓縮 `speaktype-windows-x64.zip` 後執行 `speaktype.exe`。

程式沒有程式碼簽章，SmartScreen 若跳出警告，按「其他資訊 → 仍要執行」。

## 要選哪個版本（CPU / Vulkan / CUDA）

| 版本 | 適合 | 安裝程式大小 |
|---|---|---|
| CPU（`speaktype-setup-windows-x64.exe`） | 所有電腦，最小 | 約 13 MB |
| Vulkan（`speaktype-vulkan-setup-windows-x64.exe`） | 有 NVIDIA／AMD／Intel 顯示卡 | 約 17.7 MB |
| CUDA（`speaktype-cuda-setup-windows-x64.exe`） | NVIDIA GTX 10 系列以上、驅動 R525+；安裝時另從 NVIDIA 官方下載約 430 MB | 見 Release 頁 |

GPU 版在找不到驅動程式或元件時會自動改用 CPU；「設定 → 辨識硬體」可選 自動／GPU／CPU，並顯示目前使用的後端。

## 2. 下載模型

1. 開啟主視窗 →「模型中心」。
2. 選擇模型（預設推薦 `large-v3-turbo (量化 q5_0)`，約 547 MB），按下載並確認。
3. 下載期間請勿關閉程式；中斷後再次下載會續傳。下載完成會自動校驗 SHA256（來源有提供時）。

模型位置：`%LOCALAPPDATA%\SpeakType\models`。若設定的資料夾無法寫入，程式會自動改用這個位置；舊位置裡已有的模型照樣會被使用。

## 3. 開始輸入

1. 點一下想輸入的地方（記事本、瀏覽器、LINE…）。
2. 按住 `Ctrl+Shift+L` 說話，放開後等待辨識，文字會送到原本的視窗。
3. 螢幕下方的小膠囊顯示狀態：紅點＋音量＝錄音中，「轉寫中」＝辨識中。

關閉或最小化主視窗會縮到系統匣，快捷鍵仍可使用。

### 快捷鍵與模式

| 設定 | 預設 | 說明 |
|---|---|---|
| 錄音快捷鍵 | `Ctrl+Shift+L` | 可在「設定 → 快捷鍵」修改 |
| 按住錄音，放開後轉錄 | 開 | 關閉後變成「按一下開始、再按一下結束」 |
| 啟用全域快捷鍵 | 開 | 關閉後只能在主視窗操作 |

### 語音指令

「設定」中勾選「啟用語音指令」後，單獨說出以下詞語會執行動作：

| 說 | 動作 |
|---|---|
| 刪掉／刪除 | 刪除 |
| 換行 | 換行 |
| 下一段／新段落 | 新段落 |
| 句號、逗號 | 輸入標點 |

說話時也可以說「刪掉上一句」「scratch that」取消前一句，或「三點，不對，四點」自動更正為「四點」。

## 4. 設定說明

| 區塊 | 主要選項 |
|---|---|
| 輸入裝置 | 選擇麥克風、錄音增益、即時音量、刷新裝置 |
| 轉錄模式 | 穩定／快速（先顯示草稿）、短句加速、語言（預設自動） |
| 輸出處理 | 自動標點、去除語助詞、修正口誤、整理條列、中英文之間加空白、簡繁與用語轉換（預設輸出繁體） |
| 情境 | 聊天／寫作／程式碼三組輸出規則，依前景程式自動選擇 |
| 自訂詞庫 | 專有名詞：同時當辨識提示與輸出替換；「保護」可避免被簡繁轉換 |
| 文字送出 | 貼上 (Ctrl+V，預設)、貼上 (Ctrl+Shift+V，終端機)、Unicode 打字（cmd／遠端桌面／輸入法開啟時）、只複製；送出前切回原視窗；還原剪貼簿 |
| 速度、送出與 AI | 依程式（例如 `LINE.exe`）指定送出方式、情境、AI 語氣、換行改送 Shift+Enter |
| AI 潤稿（選用） | 本機 Ollama／LM Studio 或 OpenAI 相容 API；逾時自動改用本機整理結果 |
| 錄音檔 | 保留天數（預設 30 天）、容量上限（預設 4096 MB） |
| Windows 啟動 | 登入後自動啟動、自動啟動時直接進入系統匣 |
| 外觀主題 | 介面配色 |
| 診斷 | 匯出診斷包、檢視錯誤紀錄 |

設定變更會立即儲存到 `%LOCALAPPDATA%\SpeakType\config\config.toml`。

## 5. 資料位置

| 內容 | 位置 |
|---|---|
| 設定 | `%LOCALAPPDATA%\SpeakType\config\config.toml` |
| 辨識紀錄 | `%LOCALAPPDATA%\SpeakType\data\history.json` |
| 錄音檔 | `%LOCALAPPDATA%\SpeakType\recordings\` |
| 模型 | `%LOCALAPPDATA%\SpeakType\models\` |
| 記錄檔 | `%LOCALAPPDATA%\SpeakType\logs\` |

解除安裝時會詢問是否刪除以上資料；靜默解除安裝需加 `/PURGE` 才會刪除。

## 6. 疑難排解

| 問題 | 處理方式 |
|---|---|
| 下載模型「存取被拒」 | 更新到 v0.5.4 以上；不要把模型資料夾設在 `C:\Program Files`；防毒軟體可能鎖住 `.bin.part`，可暫時將 `%LOCALAPPDATA%\SpeakType\models` 加入排除 |
| 下載失敗／很慢 | 確認可連到 `huggingface.co`；公司網路或代理可能擋下，錯誤訊息會顯示 HTTP 狀態 |
| 沒有聲音、辨識結果空白 | 選對麥克風並看即時音量；Windows「設定 → 隱私權與安全性 → 麥克風」允許桌面應用程式 |
| 快捷鍵沒反應 | 確認「啟用全域快捷鍵」；快捷鍵可能與其他程式衝突，改用其他組合 |
| 文字沒有送出 | 目標程式以系統管理員執行時，SpeakType 也需以系統管理員執行；終端機改用 Ctrl+Shift+V 或 Unicode 打字 |
| 聊天軟體換行直接送出 | 在「速度、送出與 AI」為該程式勾選換行改送 Shift+Enter |
| 防毒誤報 | 程式未簽章且會模擬按鍵；請用 SHA256 確認檔案來自官方 Release |
| 回報問題 | 「診斷 → 匯出診斷包」，附在 [Issues](https://github.com/stevenke1981/speaktype/issues) |
