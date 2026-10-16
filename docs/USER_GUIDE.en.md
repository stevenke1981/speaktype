# SpeakType user guide (v0.5.1-test)

[繁體中文](USER_GUIDE.zh-TW.md) | [Back to README](../README.en.md)

## Install
1. Download `speaktype-setup-windows-x64.exe` from [Releases](https://github.com/stevenke1981/speaktype/releases).
2. Verify: `Get-FileHash .\speaktype-setup-windows-x64.exe -Algorithm SHA256` and compare with `SHA256SUMS.txt`.
3. Run it: "install for me only" (no admin) or all users (`C:\Program Files\SpeakType`); optional start at sign-in.
4. Portable: unzip `speaktype-windows-x64.zip` and run `speaktype.exe`. Not code-signed; SmartScreen may warn.

## Download a model
Main window → Model Center → pick a model (default `large-v3-turbo` q5_0, ~547 MB) → download. Downloads resume and are SHA256-checked when the source provides a hash. Location: `%LOCALAPPDATA%\SpeakType\models` (used automatically if the configured folder is not writable; existing models in old locations are still used).

## Dictate
Click into a text field, hold `Ctrl+Shift+L`, speak, release. The pill at the bottom of the screen shows recording / transcribing. Closing the window sends it to the tray; the hotkey keeps working. Untick "hold to record" for press-to-start / press-to-stop.

Voice commands (when enabled): 刪掉/刪除 (delete), 換行 (newline), 新段落 (new paragraph), 句號/逗號 (punctuation); "scratch that" drops the previous sentence.

## Settings
Input device & gain · transcription mode (stable/fast), language (auto) · output cleanup (punctuation, fillers, self-corrections, lists, CJK spacing, Traditional Chinese) · scenarios (chat/writing/code) · custom vocabulary · text sending (Ctrl+V, Ctrl+Shift+V, Unicode typing, copy only) · per-app rules · optional AI cleanup (Ollama / OpenAI-compatible) · recordings retention (30 days / 4096 MB) · startup · theme · diagnostics. Saved to `%LOCALAPPDATA%\SpeakType\config\config.toml`.

## Troubleshooting
- **Access denied when downloading a model**: update to v0.5.1-test+, keep the model folder out of `C:\Program Files`, exclude it from antivirus if `.bin.part` is locked.
- **No audio**: choose the right mic; allow desktop apps microphone access in Windows privacy settings.
- **No text inserted**: run SpeakType as admin if the target app is admin; use Ctrl+Shift+V or Unicode typing for terminals.
- **Report a bug**: Diagnostics → export bundle, attach to an [issue](https://github.com/stevenke1981/speaktype/issues).
