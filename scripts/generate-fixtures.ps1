param([string]$Ffmpeg = 'ffmpeg')
$ErrorActionPreference = 'Stop'
. "$PSScriptRoot/common.ps1"

$fixtureDir = Join-Path (Split-Path -Parent $PSScriptRoot) 'tests/fixtures'
New-Item -ItemType Directory -Force $fixtureDir | Out-Null
$common = @('-hide_banner', '-loglevel', 'error', '-y', '-f', 'lavfi', '-i', 'testsrc2=size=160x96:rate=24')
$h264 = @('-c:v', 'libx264', '-preset', 'medium', '-pix_fmt', 'yuv420p', '-g', '12', '-keyint_min', '12',
    '-bf', '2', '-x264-params', 'scenecut=0:open-gop=0:threads=1', '-video_track_timescale', '12288')

Invoke-CheckedNative $Ffmpeg ($common + @('-f', 'lavfi', '-i', 'sine=frequency=440:sample_rate=48000', '-t', '3') +
    $h264 + @('-c:a', 'aac', '-b:a', '64k', '-timecode', '01:00:00:00', "$fixtureDir/h264-main.mp4"))
Invoke-CheckedNative $Ffmpeg ($common + @('-t', '1', '-vf', 'hue=h=90', '-an') +
    $h264 + @('-timecode', '01:00:01:00', "$fixtureDir/h264-replacement.mp4"))
Invoke-CheckedNative $Ffmpeg @('-hide_banner', '-loglevel', 'error', '-y', '-i', "$fixtureDir/h264-main.mp4",
    '-map', '0:v', '-map', '0:a', '-c', 'copy', "$fixtureDir/h264-audio.mkv")
Invoke-CheckedNative $Ffmpeg ($common + @('-t', '2', '-an', '-c:v', 'libx265', '-preset', 'fast',
    '-pix_fmt', 'yuv420p10le', '-x265-params', 'keyint=12:min-keyint=12:scenecut=0:open-gop=0:bframes=2:pools=none:frame-threads=1:info=0:log-level=error',
    '-tag:v', 'hvc1', '-video_track_timescale', '12288', "$fixtureDir/hevc-main10.mp4"))

$manifest = [ordered]@{
    generator = (& $Ffmpeg -version | Select-Object -First 1)
    source = 'FFmpeg lavfi testsrc2 and sine; no external media'
    files = @(Get-ChildItem $fixtureDir -File | Where-Object Extension -In '.mp4', '.mkv' |
        Sort-Object Name | ForEach-Object {
            [ordered]@{ name = $_.Name; bytes = $_.Length; sha256 = (Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant() }
        })
}
$manifest | ConvertTo-Json -Depth 4 | Set-Content "$fixtureDir/manifest.json" -Encoding utf8
