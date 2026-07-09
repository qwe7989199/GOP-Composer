param(
    [string]$FfmpegDir = $env:FFMPEG_DIR,
    [string]$MkvSample = $env:GOPCOMPOSER_MKV_SAMPLE,
    [string]$HevcSample = $env:GOPCOMPOSER_HEVC_SAMPLE
)

$ErrorActionPreference = "Stop"

$projectRoot = Split-Path -Parent $PSScriptRoot
if ([string]::IsNullOrWhiteSpace($FfmpegDir)) {
    $FfmpegDir = Join-Path $projectRoot "vendor\ffmpeg-gopcomposer-win64-msvc"
}
$exe = Join-Path $projectRoot "target\release\gop-composer.exe"
$ffprobe = Join-Path $FfmpegDir "bin\ffprobe.exe"
$out = Join-Path $env:TEMP "gopcomposer_smoke_gops_2_5.mp4"

if (!(Test-Path $FfmpegDir)) {
    throw "FFmpeg directory not found: $FfmpegDir"
}
if (!(Test-Path $ffprobe)) {
    throw "ffprobe not found: $ffprobe"
}
if ([string]::IsNullOrWhiteSpace($MkvSample) -or !(Test-Path $MkvSample)) {
    throw "MKV sample not found: $MkvSample"
}
if ([string]::IsNullOrWhiteSpace($HevcSample) -or !(Test-Path $HevcSample)) {
    throw "HEVC sample not found: $HevcSample"
}

Push-Location $projectRoot
try {
    $env:FFMPEG_DIR = $FfmpegDir
    cargo build --offline --release

    & $exe --probe $MkvSample
    & $exe --probe $HevcSample
    & $exe --export-gops $MkvSample "2,5" mp4 $out

    $streams = & $ffprobe -v error -show_entries stream=index,codec_type,codec_name,nb_read_packets -count_packets -of compact=p=0:nk=1 $out
    $streams | Write-Output
    if (!($streams -match "\|video\|") -or !($streams -match "\|audio\|")) {
        throw "Export smoke failed: expected video and audio streams in $out"
    }
} finally {
    Pop-Location
}
