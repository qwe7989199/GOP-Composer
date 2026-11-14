param(
    [string]$FfmpegDir = $env:FFMPEG_DIR,
    [string]$Executable = '',
    [string]$Ffmpeg = 'ffmpeg',
    [string]$Ffprobe = 'ffprobe',
    [switch]$SkipBuild
)
$ErrorActionPreference = 'Stop'
. "$PSScriptRoot/common.ps1"
$projectRoot = Split-Path -Parent $PSScriptRoot
if (!$Executable) { $Executable = Join-Path $projectRoot 'target/release/gop-composer.exe' }
$fixtures = Join-Path $projectRoot 'tests/fixtures'
$scratch = Join-Path ([System.IO.Path]::GetTempPath()) ('gop-composer-smoke-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory $scratch | Out-Null

function Get-VideoPackets([string]$Path) {
    $json = Invoke-CheckedNative $Ffprobe @('-v', 'error', '-select_streams', 'v:0', '-show_packets',
        '-show_data_hash', 'sha256', '-of', 'json', $Path)
    return ($json | Out-String | ConvertFrom-Json).packets
}

function Get-FrameHashes([string]$Path) {
    $lines = Invoke-CheckedNative $Ffmpeg @('-v', 'error', '-xerror', '-threads', '1', '-i', $Path,
        '-map', '0:v:0', '-fps_mode', 'passthrough', '-f', 'framemd5', '-')
    return @($lines | Where-Object { $_ -match '^0,' } | ForEach-Object { ($_ -split ',')[-1].Trim() })
}

Push-Location $projectRoot
$previousFfmpegDir = $env:FFMPEG_DIR
try {
    if ($FfmpegDir) { $env:FFMPEG_DIR = $FfmpegDir }
    if (!$SkipBuild) { Invoke-CheckedNative cargo @('build', '--release', '--locked') }
    foreach ($file in (Get-Content "$fixtures/manifest.json" -Raw | ConvertFrom-Json).files) {
        if ((Get-FileHash (Join-Path $fixtures $file.name) -Algorithm SHA256).Hash -ne $file.sha256) {
            throw "Fixture hash mismatch: $($file.name)"
        }
    }
    Invoke-CheckedNative $Executable @('--probe', "$fixtures/h264-audio.mkv")
    Invoke-CheckedNative $Executable @('--probe', "$fixtures/hevc-main10.mp4")
    Invoke-CheckedNative $Executable @('--decode-window', "$fixtures/hevc-main10.mp4", '10', '8')

    $selected = Join-Path $scratch 'selected.mp4'
    Invoke-CheckedNative $Executable @('--export-gops', "$fixtures/h264-audio.mkv", '2,5', 'mp4', $selected)
    $streams = (Invoke-CheckedNative $Ffprobe @('-v', 'error', '-show_streams', '-of', 'json', $selected) |
        Out-String | ConvertFrom-Json).streams
    if (!($streams | Where-Object codec_type -EQ 'audio')) { throw 'AAC stream was lost' }
    if (@(Get-VideoPackets $selected).Count -ne 24) { throw 'Selected GOP frame count differs' }
    if (@(Get-FrameHashes $selected).Count -ne 24) { throw 'Selected GOP decode failed' }

    foreach ($name in @('h264-main.mp4', 'hevc-main10.mp4')) {
        $source = Join-Path $fixtures $name
        $sourcePackets = @(Get-VideoPackets $source)
        $sourceFrames = @(Get-FrameHashes $source)
        $expectedIndices = @(12..23) + @(0..11) + @(12..23)
        foreach ($generatePts in @($false, $true)) {
            $ordered = Join-Path $scratch 'ordered.mp4'
            $exportArgs = @('--export-gops-ordered', $source, '2,1,2', 'mp4', $ordered)
            if ($generatePts) { $exportArgs += '--generate-pts' }
            Invoke-CheckedNative $Executable $exportArgs
            $packets = @(Get-VideoPackets $ordered)
            $frames = @(Get-FrameHashes $ordered)
            if ($packets.Count -ne 36 -or $frames.Count -ne 36) { throw 'Ordered export must have 36 frames' }
            for ($i = 0; $i -lt 36; $i++) {
                if ($packets[$i].data_hash -ne $sourcePackets[$expectedIndices[$i]].data_hash) {
                    throw "Compressed video packet changed at $i ($name)"
                }
                if ($frames[$i] -ne $sourceFrames[$expectedIndices[$i]]) {
                    throw "Decoded picture changed at $i ($name)"
                }
                if ($i -gt 0) {
                    $step = [double]::Parse($packets[$i].dts_time, [cultureinfo]::InvariantCulture) -
                        [double]::Parse($packets[$i - 1].dts_time, [cultureinfo]::InvariantCulture)
                    if ([math]::Abs($step - 1.0 / 24.0) -gt 0.00001) { throw "Timestamp gap at packet $i" }
                }
            }
        }
    }
    Write-Output 'PASS: fixture integrity, HEVC preview, audio copy, packet hashes, decoded pictures and continuous DTS.'
} finally {
    $env:FFMPEG_DIR = $previousFfmpegDir
    Pop-Location
    # This directory is created exclusively by this invocation under the OS temp directory.
    $resolvedScratch = [System.IO.Path]::GetFullPath($scratch)
    $tempRoot = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath())
    if (!$resolvedScratch.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase) -or
        !(Split-Path -Leaf $resolvedScratch).StartsWith('gop-composer-smoke-')) {
        throw "Unexpected smoke directory: $resolvedScratch"
    }
    Remove-Item -LiteralPath $resolvedScratch -Recurse -Force
}
