# SpeakType Packaging

Release builds have separate CPU, CUDA and Vulkan jobs in GitHub Actions (`.github/workflows/release.yml`):
`cargo build --release --locked --bins` (plus `--features cuda` or `--features vulkan`), then
`scripts/build-installer.ps1` (Inno Setup) and `scripts/package-portable.ps1`.

Published assets are preserved as separate CPU, CUDA and Vulkan groups (installer, ZIP,
and their checksum manifest). Before publishing, `scripts/release_preflight.py`
validates the local manifest and checks the entire remote group. An empty group
may be uploaded. A complete group is downloaded publicly and compared with every
local file, including the manifest; only identical bytes allow a successful no-op.
A partial group, content conflict, or lookup/download error stops publication.
The workflow never fills a partial group with newly rebuilt files, replaces old
assets, or moves an existing tag. Releases for the same ref are serialized.

If an upload is interrupted, keep the partial release for investigation and use a
fresh version/tag for corrected artifacts. Do not delete old assets to bypass the
guard. Repackaging the same source can change ZIP bytes and therefore intentionally
fails against an existing group. Tags must match the Cargo package version (an
optional prerelease suffix such as `-test` is allowed). Verify final public downloads
against `SHA256SUMS.txt` / `SHA256SUMS-cuda.txt` / `SHA256SUMS-vulkan.txt` before announcing a release.

For a local NVIDIA build use `cargo build --release --features cuda` (or `--features vulkan` for any GPU with the Vulkan SDK installed) and copy the CUDA runtime DLLs next to `speaktype.exe`; the installer picks up any `*.dll` in `target\release`.

## Build Notes

### Prerequisites

- Rust toolchain (stable)
- Visual Studio 2022+ with C++ toolchain
- CMake 3.21+
- NVIDIA CUDA Toolkit 12.x (optional, for GPU inference)

### CUDA + Visual Studio 2026 Known Issue

CUDA 12.6 does not officially support Visual Studio 2026 (MSVC 19.50+, `_MSC_VER >= 1950`). If you encounter:

```
fatal error C1189: #error: -- unsupported Microsoft Visual Studio version!
```

Set the following environment variable before `cargo build` to work around the version check:

```powershell
$env:CL = "/D_MSC_VER=1949"
$env:CMAKE_CUDA_ARCHITECTURES = "86"   # adjust to your GPU's compute capability
cargo build --release
```

- `CL` tells MSVC to report an older `_MSC_VER` that CUDA accepts (≥1910, <1950) while keeping MSVC 2026's STL happy (expects ≥1944).
- `CMAKE_CUDA_ARCHITECTURES` avoids CMake 4.3's `native` GPU detection.

To find your GPU's compute capability:

```powershell
nvidia-smi --query-gpu=name,compute_cap --format=csv,noheader
```

After the first successful build, CUDA build artifacts are cached and the env vars are not needed for subsequent builds unless you run `cargo clean`.

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

The Release folder, portable ZIP, and installer include the app executable, the matching CUDA cuBLAS runtime DLLs, and docs. They do not include model files, recordings, logs, or diagnostic bundles. A compatible NVIDIA display driver is still required for CUDA inference.

The installer creates Start Menu and Desktop shortcuts. The optional startup component writes `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\SpeakType` with `--tray`, so SpeakType launches directly into the system tray after Windows login.
