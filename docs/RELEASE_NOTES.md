SpeakType 0.4.0 (test build from the `test` branch)

**Downloads:** `speaktype-setup-windows-x64.exe` (installer, zh-TW / en, per-user or all users, optional autostart) or `speaktype-windows-x64.zip` (portable). CPU build; the speech model (~547 MB) is downloaded on first start.

**Faster:** 2–3× faster transcription on CPU (reused whisper state, clip-sized encoder window, all cores, warm-up), mic kept warm with 300 ms pre-roll, hotkey reacts immediately even when minimised to the tray, no blocking GPU query at startup, quantized default model.

**Text sending:** per-app paste / Ctrl+Shift+V / SendInput Unicode, clipboard (text + image) restored in the background, waits for modifier release, re-focuses the original window, Shift+Enter newlines for chat apps.

**Typeless-style cleanup:** filler removal, self-correction, numbered lists, CJK/Latin spacing, Traditional Chinese by default, language auto-detect for zh/en mix, optional AI rewrite (Ollama / OpenAI-compatible), per-app tone, history panel with latency, floating recording indicator, toggle mode.

**Not verified on real hardware yet:** microphone capture and global hotkey on a physical Windows PC, insertion into LINE / Office / terminals with an IME active, the floating indicator on multi-monitor setups.
