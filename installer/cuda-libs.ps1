# Downloads the NVIDIA CUDA 12.6 runtime DLLs SpeakType's CUDA build needs
# (cudart64_12.dll, cublas64_12.dll, cublasLt64_12.dll) into -Dest.
# Source: NVIDIA redistributable archives (developer.download.nvidia.com),
# verified against the SHA256 published in NVIDIA's redistrib JSON.
# Fallback: the speaktype-cuda-libs zip on the GitHub release (SHA256 pinned below).
# Exit codes: 0 ok, 1 download/verify failed (app falls back to CPU).
param(
    [string]$Dest = $PSScriptRoot,
    [string]$CudaRedist = '12.6.3'
)
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'Continue'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
$need = 'cudart64_12.dll', 'cublas64_12.dll', 'cublasLt64_12.dll'
$base = 'https://developer.download.nvidia.com/compute/cuda/redist'
$mirror = 'https://github.com/stevenke1981/speaktype/releases/download/cuda-libs-12.6/speaktype-cuda-libs-12.6-windows-x64.zip'
$mirrorSha = '@MIRROR_SHA256@'
$tmp = Join-Path $env:TEMP ("speaktype-cuda-" + [guid]::NewGuid())
New-Item -ItemType Directory -Force $tmp, $Dest | Out-Null

function Have-All { -not ($need | Where-Object { -not (Test-Path (Join-Path $Dest $_)) }) }
function Get-Verified([string]$url, [string]$sha, [string]$out) {
    for ($i = 1; $i -le 3; $i++) {
        try {
            Write-Host "下載 $url (第 $i 次)"
            $wc = New-Object Net.WebClient
            $wc.DownloadFile($url, $out)
            $h = (Get-FileHash $out -Algorithm SHA256).Hash.ToLower()
            if ($h -eq $sha.ToLower()) { Write-Host "SHA256 OK $h"; return }
            Write-Warning "SHA256 不符: $h (預期 $sha)"
        } catch { Write-Warning $_.Exception.Message }
        Start-Sleep -Seconds (2 * $i)
    }
    throw "下載或校驗失敗: $url"
}
function Extract-Needed([string]$zip) {
    $x = Join-Path $tmp ([IO.Path]::GetFileNameWithoutExtension($zip))
    Expand-Archive -Force $zip $x
    foreach ($n in $need) {
        $f = Get-ChildItem -Recurse -File $x -Filter $n | Select-Object -First 1
        if ($f) { Copy-Item -Force $f.FullName $Dest }
    }
}

try {
    if (Have-All) { Write-Host 'CUDA 元件已存在'; exit 0 }
    try {
        $json = Invoke-RestMethod "$base/redistrib_$CudaRedist.json"
        foreach ($pkg in 'cuda_cudart', 'libcublas') {
            $w = $json.$pkg.'windows-x86_64'
            $zip = Join-Path $tmp ([IO.Path]::GetFileName($w.relative_path))
            Get-Verified "$base/$($w.relative_path)" $w.sha256 $zip
            Extract-Needed $zip
        }
    } catch {
        Write-Warning "NVIDIA 來源失敗：$($_.Exception.Message)；改用 GitHub 鏡像"
        if ($mirrorSha -notmatch '^[0-9a-f]{64}$') { throw }
        $zip = Join-Path $tmp 'mirror.zip'
        Get-Verified $mirror $mirrorSha $zip
        Extract-Needed $zip
    }
    if (-not (Have-All)) { throw '解壓縮後仍缺少 CUDA DLL' }
    Write-Host "CUDA 元件安裝完成：$Dest"
    exit 0
} catch {
    Write-Host "CUDA 元件安裝失敗：$($_.Exception.Message)" -ForegroundColor Red
    Write-Host 'SpeakType 仍可使用 CPU 辨識；之後可在「設定 → 辨識硬體」重新下載。'
    exit 1
} finally {
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}
