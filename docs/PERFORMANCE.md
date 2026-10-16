# Performance notes (v0.4.0)

Measured with `speaktype-bench` (no microphone needed) on an 8-core Xeon VM, CPU only.
Raw logs: `latency-before.txt` / `latency-after.txt` in the release evidence.

## Where the time went in 0.3.1

| Stage | 0.3.1 | Problem |
|---|---|---|
| Hotkey edge → UI reacts | 0–500 ms | Hotkey was polled from the egui frame loop, which repaints every 500 ms while minimised to the tray |
| Hotkey press → mic open | 100–400 ms (Windows WASAPI) | Stream opened on every press; first syllable often lost |
| Save WAV (32-bit float, 48 kHz stereo) | 15–20 ms | On the critical path before transcription |
| VAD | — | Run on interleaved 48 kHz stereo with 16 kHz frame sizes (wrong) |
| whisper | 20.2–21.0 s (turbo, 13 s clip) | New state per call, full 30 s encoder window, 4 threads, language forced to `zh` |
| Insert | 120 ms fixed sleeps | + clipboard restore raced slow apps (stale paste) |
| Startup | +0.3–2 s on Windows | `wmic` / PowerShell GPU query on the UI thread (`wmic` is gone on Windows 11 24H2) |

## Changes

1. **Hotkey waker** – the listener thread calls `ctx.request_repaint()` on every edge.
2. **Warm microphone** (`recording.keep_mic_warm`, default on) – stream stays open, 300 ms pre-roll ring buffer.
3. **Audio prep once** – downmix + resample to 16 kHz mono, normalise, energy VAD with 200 ms padding.
4. **whisper** – one reusable `WhisperState`, `audio_ctx` sized to the clip (`transcription.fast_encoder`), `no_timestamps`, `single_segment`, all cores (max 8), language `auto` (correct English, zh-TW/en mixed), Traditional-Chinese prompt + dictionary terms, 1 s warm-up after loading.
5. **Quantized default model** – `large-v3-turbo-q5_0` (547 MB, loads in 0.5 s vs 1.4–2.2 s).
6. **WAV after delivery**, 16-bit PCM, on a background thread; history written on a background thread.
7. **Insertion** – `SendInput` chord, 15 ms paste delay, clipboard restore after 300 ms on a background thread guarded by `GetClipboardSequenceNumber`.
8. **Startup** – GPU name detected on a background thread.

## Results (hotkey release → text handed to the target app)

| Model / clip | 0.3.1 | 0.4.0 |
|---|---|---|
| turbo fp16, zh 12.9 s | 20.3–21.2 s | 7.7–9.9 s |
| turbo q5_0, zh 12.9 s | n/a | 6.7–9.7 s |
| turbo fp16, en 10.9 s | 20.6–20.8 s (translated to Chinese) | 6.5–8.1 s |
| base, en 10.9 s | 3.5–3.7 s (garbage) | 0.38–0.45 s |
| base, zh 12.9 s | 1.29–1.31 s | 0.53–0.59 s |

`audio_ctx` alone: turbo q5_0 zh 11.9–19.5 s without vs 6.5–8.7 s with. Threads: 4 → 12.6–16.5 s, 8 → 6.5–8.7 s.

The VM is shared, so run-to-run noise is ±25 %. A GPU build (`--features cuda`) brings turbo to well under a second.
