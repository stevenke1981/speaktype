# SpeakType Packaging

SpeakType stores user data in `%LOCALAPPDATA%\SpeakType\`.

Folder layout:

- `config`: app settings, custom vocabulary, output rules, startup settings
- `models`: downloaded Whisper models
- `recordings`: saved WAV recordings
- `logs`: runtime logs
- `diagnostics`: exported diagnostic bundles

Release options:

- Portable ZIP: run `powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\package-portable.ps1`
- NSIS installer: install NSIS, then run `powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\build-installer.ps1`

The portable ZIP and installer include the app executable and docs only. They do not include model files, recordings, logs, or diagnostic bundles.

The installer creates Start Menu and Desktop shortcuts. The optional startup component writes `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\SpeakType` with `--tray`, so SpeakType launches directly into the system tray after Windows login.
