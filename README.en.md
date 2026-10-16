# SpeakType

[繁體中文](README.md)

Local voice typing for Windows: hold a hotkey, speak, release — the text appears in whatever app has the cursor (Chrome, Office, Notepad, LINE, terminals…). Speech recognition runs locally with [whisper.cpp](https://github.com/ggml-org/whisper.cpp); AI cleanup is optional and can use a local Ollama server or a cloud API.

![Main window](docs/images/win-main.png)

📖 User guide: [docs/USER_GUIDE.en.md](docs/USER_GUIDE.en.md) · Changelog: [CHANGELOG.md](CHANGELOG.md)

> Latest build: **v0.5.1-test** (prerelease from the `test` branch), Windows x64 only.

## Download & install

From [Releases](https://github.com/stevenke1981/speaktype/releases):

- `speaktype-setup-windows-x64.exe` — installer (recommended; zh-TW / English, "install for me only" without admin, optional start at sign-in)
- `speaktype-windows-x64.zip` — portable build, run `speaktype.exe`
- `SHA256SUMS.txt` — checksums. Verify: `Get-FileHash .\speaktype-setup-windows-x64.exe -Algorithm SHA256`

The binaries are not code-signed, so SmartScreen may warn ("More info → Run anyway").

## Quick start

1. Start SpeakType.
2. Download a speech model in the Model Center (default `large-v3-turbo` q5_0, about 547 MB).
3. Click into any text field, hold `Ctrl+Shift+L`, speak, release.

## Models

New models are downloaded from Hugging Face to `%LOCALAPPDATA%\SpeakType\models`. Since v0.5.1, if the configured folder is not writable (e.g. under `C:\Program Files`) SpeakType falls back to that per-user folder; models already present in old locations (configured folder, `models\` next to the exe) are still used.

| Model | Size | Notes |
|---|---|---|
| large-v3-turbo (q5_0) — default | ~547 MB | near full quality, fast to load |
| large-v3-turbo | ~1.6 GB | best quality, NVIDIA GPU recommended |
| medium | ~1.5 GB | balanced |
| small | ~488 MB | faster |
| small (q5_1) | ~181 MB | fastest on low-end CPUs |
| base | ~142 MB | very fast, lower accuracy (mainly English) |

## Usage

| Action | What it does |
|---|---|
| Hold `Ctrl+Shift+L`, speak, release | Push-to-talk (default) |
| Press once to start, again to stop | Toggle mode: Settings → untick "hold to record" |
| Small pill at the bottom of the screen | Red dot + level while recording, "transcribing" afterwards |
| Main window → History | Search, copy, latency and target app of each dictation |

## Features vs. Typeless

| Typeless feature | SpeakType | Notes |
|---|---|---|
| Push-to-talk / toggle hotkey | ✅ | Both modes, custom hotkey |
| Filler removal (um, uh, 嗯, 呃) | ✅ local | Only leading / stand-alone fillers, never words like "likely" or 這個方案 |
| Self-correction | ✅ local | "3 pm, no wait, 4 pm" → "4 pm"; 不對 / 我是說 / I mean / scratch that |
| Auto punctuation, list formatting | ✅ local | "first… second…" / 第一…第二… → numbered list |
| Personal dictionary | ✅ | Terms bias recognition (prompt) and fix spelling; can be protected from script conversion |
| Per-app tone / context | ✅ | Foreground app picks scenario (chat / writing / code), insertion method and AI tone |
| Multilingual, zh-TW/en mixed | ✅ | Language "auto"; Chinese is always output in Traditional Chinese (Taiwan), English stays English |
| History | ✅ | Last 200 dictations with latency and app |
| Minimal floating indicator | ✅ | Never steals focus, click-through |
| AI auto-edit | ✅ optional | Any OpenAI-compatible endpoint: local Ollama / LM Studio or OpenAI, Groq…; falls back to the local result on error / timeout |
| Cloud transcription | ➖ | Deliberately not: audio never leaves the PC |
| Voice editing of selected text / translation | ❌ | Not yet |
| macOS / iOS | ❌ | Windows only |

## Text insertion

| Method | Best for |
|---|---|
| Paste (Ctrl+V) — default | Chrome, Office, Notepad, LINE, Windows Terminal. The clipboard (text or image) is saved and restored in the background, unless you changed it meanwhile |
| Paste (Ctrl+Shift+V) | mintty / Git Bash |
| Unicode typing | cmd / PowerShell consoles, Remote Desktop. One batched `SendInput` call, bypasses the IME, handles CJK and emoji, never touches the clipboard |
| Copy only | Clipboard only |

Before sending, SpeakType waits until Ctrl/Shift/Alt are released (no accidental Ctrl+Shift+V) and re-focuses the window that was active when you released the hotkey. Per-app rules (Settings → "Speed, sending and AI") set the method, scenario, AI tone and whether newlines are sent as Shift+Enter (so chat apps don't send early).

> Windows blocks synthetic input into apps running as administrator; run SpeakType elevated too if you need that.

## AI cleanup (optional)

Settings → "AI cleanup": enable, click "Local Ollama" (after `ollama pull qwen2.5:3b`) or enter any OpenAI-compatible endpoint and key. The key is stored in plain text in `config.toml`; `SPEAKTYPE_AI_API_KEY` works too.

## Speed

Hotkey release → text sent, 8-core CPU without GPU, 11–13 s clips:

| Model | 0.3.1 | 0.4.0 |
|---|---|---|
| large-v3-turbo, Chinese | 20.2–21.0 s | 7.7–9.9 s |
| large-v3-turbo q5_0 (new default), Chinese | — | 6.7–9.7 s |
| base, English | 3.4–3.6 s (forced to Chinese, wrong output) | 0.38–0.45 s (correct English) |

See [docs/PERFORMANCE.md](docs/PERFORMANCE.md). NVIDIA users can build `cargo build --release --features cuda` (see [PACKAGING.md](PACKAGING.md)).

## Screenshots

| Settings | Speed, sending & AI | History |
|---|---|---|
| ![](docs/images/win-settings.png) | ![](docs/images/win-settings-speed.png) | ![](docs/images/win-history.png) |

## Troubleshooting

- **Model download fails with "Access denied"**: update to v0.5.1-test or later (downloads go to `%LOCALAPPDATA%\SpeakType\models`). Don't set the model folder under `C:\Program Files`. Antivirus may also lock the `.bin.part` file.
- **Hotkey works but no text appears**: if the target app runs as administrator, run SpeakType as administrator too.
- **No audio**: pick the right microphone in Settings → Input device, and allow desktop apps to use the microphone in Windows privacy settings.

Idle measurements (CI, 30 s): Windows CPU 47.8% of one core → 0%, working set 151 → 154 MB; Linux CPU 0% → 0%, RSS 174 → 40–48 MB. Windows exe grew 19.1 → 36.7 MB (bundled Skia).

## UI (v0.5)

Since v0.5 the UI is built with [Slint](https://slint.dev) instead of egui. On Windows it renders with Skia and falls back to Skia software, then to Slint's software renderer. CJK font chain: Microsoft JhengHei UI → Microsoft YaHei UI → PMingLiU → SimSun.

- Event-driven, so there is no redraw while idle. Idle CPU over 30 s in CI: Windows went from 47.8% of one core (egui) to 0%; Linux stays at 0%.
- The recording indicator is now its own small window: frameless, transparent, always on top, does not take focus, not shown in the taskbar. The dark box that used to appear behind the pill is gone.
- Closing or minimising the window sends it to the tray; hotkeys keep working in the background.

## Build from source

Rust stable, CMake, LLVM/Clang and the Visual Studio C++ tools.

```powershell
cargo build --release
cargo test --release
./scripts/build-installer.ps1      # Inno Setup installer -> dist\
./scripts/package-portable.ps1     # portable zip -> dist\
target\release\speaktype-bench.exe models\ggml-base.bin sample.wav 3
```

## Data

`%LOCALAPPDATA%\SpeakType\`: `config\config.toml`, `data\history.json`, `recordings\`, `models\`, `logs\`. The uninstaller asks whether to delete it (silent uninstall only with `/PURGE`).
