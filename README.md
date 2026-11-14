# GOP Composer

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-Windows%20first-8ab4f8.svg)](#platform-status)
[![Codecs](https://img.shields.io/badge/codecs-H.264%20%7C%20HEVC-7bc8a4.svg)](#compatibility)

GOP Composer is a local desktop tool for GOP-level video replacement and remuxing.
It is designed for finished H.264/HEVC delivery streams where a small visual
change should not require re-encoding the entire program.

[中文说明](README.zh-CN.md)

## Download

Download the **Windows x64 ZIP** from [GitHub Releases](https://github.com/qwe7989199/GOP-Composer/releases/latest),
extract the entire archive, and run `gop-composer.exe`. Keep the bundled DLLs next
to the executable. No separate FFmpeg installation is needed to run this package.
The release includes SHA-256 checksums, dependency notices and matching FFmpeg source.

## At A Glance

| Area | Scope |
| --- | --- |
| Primary use case | Replace a small number of GOPs in an existing delivery stream |
| Processing model | Copy compressed packets, remux the final main track |
| Target codecs | H.264 and HEVC |
| Main outputs | MP4 and raw H.264/HEVC Annex B |
| Privacy model | Fully local processing |
| Current platform focus | Windows desktop |

## Why

Distribution video often has to be regenerated because of a tiny late-stage
change: a burned-in subtitle typo, a replaced shot, a logo fix, or a short
graphic update. Professional mastering workflows can solve similar problems with
incremental package formats such as IMF, but downstream delivery files are still
usually ordinary MP4, MKV, or MOV assets.

GOP Composer targets that gap.

If the affected content falls inside one GOP, or a short continuous group of
GOPs, the intended workflow is:

1. Locate the affected GOPs in the delivery stream.
2. Re-encode only the matching frame range with compatible codec settings.
3. Import the original delivery file as the main video.
4. Import the replacement clip and place its GOPs on the main track.
5. Export a new MP4 or elementary stream by copying compressed packets.

Unchanged GOPs are kept as compressed packets. They are not decoded and
re-encoded.

## Features

- Local-first desktop GUI. Media files are processed on your machine and are not
  uploaded.
- GOP indexing through FFmpeg for common delivery containers.
- H.264 and HEVC focused workflow for preview, validation, raw export, and MP4
  remuxing.
- Multi-source timeline with GOP selection, copy, cut, paste, delete, undo,
  redo, and cross-track drag insertion.
- Main-track based export: the exported video is always composed from the GOPs
  currently placed on the main track.
- MP4 export with timestamp rewriting, optional generated PTS, configurable
  timebase, and optional audio copying from the main track time ranges.
- Raw H.264/HEVC Annex B export for video-only workflows.
- Timecode-assisted replacement for quickly applying replacement GOPs to matching
  main-track GOP positions.
- English and Chinese UI, switchable from the toolbar.

## Typical Workflow

1. Click **Open Main Video** and import the delivery file.
2. Click **Add Videos** to import one or more replacement files.
3. Select GOPs on the replacement track and drag them into the main track, or
   apply matching GOPs automatically with timecode.
4. Preview GOP boundaries and playback around the replacement.
5. Export the main track to MP4 or raw H.264/HEVC.

GOP Composer does not try to hide stream compatibility problems. Replacement
sources are expected to be encoded intentionally for the target delivery stream.

## Timecode Replacement

When a replacement file contains readable timecode, GOP Composer can map
replacement GOPs to main-track GOPs by absolute timecode frame:

```text
absolute frame = timecode start frame + GOP start frame
```

If the replacement file and the main video use the same timecode rate, the app
asks whether the matched replacement GOPs should be applied to the main track.
Matched GOPs are moved from the replacement track into the corresponding main
track positions.

This is a mechanical mapping feature. The user is still responsible for making
sure the replacement clip has the correct frame range, GOP boundaries, and visual
content.

### Add Timecode With FFmpeg

If the replacement clip is already encoded correctly and only needs timecode
metadata, remux it:

```powershell
ffmpeg -y -i "replacement_no_tc.mp4" -map 0 -c copy `
  -timecode "01:00:04:00" `
  -metadata:s:v:0 timecode="01:00:04:00" `
  "replacement_tc.mp4"
```

If the replacement section is being encoded, write the timecode during encode.
The timecode should match the first main-track GOP that the replacement GOP
should replace.

H.264 example:

```powershell
ffmpeg -y -i "source_master.mov" -ss 00:12:34.000 -t 2.000 `
  -vf "subtitles=subtitle_fix.ass" -an `
  -c:v libx264 -pix_fmt yuv420p -r 24000/1001 `
  -g 24 -keyint_min 24 -x264-params "scenecut=0:open-gop=0" `
  -video_track_timescale 24000 `
  -timecode "01:00:04:00" `
  -metadata:s:v:0 timecode="01:00:04:00" `
  "replacement_tc.mp4"
```

HEVC example:

```powershell
ffmpeg -y -i "source_master.mov" -ss 00:12:34.000 -t 2.000 `
  -vf "subtitles=subtitle_fix.ass" -an `
  -c:v libx265 -pix_fmt yuv420p -r 24000/1001 `
  -x265-params "keyint=24:min-keyint=24:scenecut=0:open-gop=0" `
  -video_track_timescale 24000 `
  -timecode "01:00:04:00" `
  -metadata:s:v:0 timecode="01:00:04:00" `
  "replacement_tc.mp4"
```

## Export Model

Exports always use the main track as the ordered source of truth.

MP4 export:

- Copies compressed video packets from each GOP segment.
- Requires compatible video stream parameters across all replacement GOPs.
- Can copy audio packets whose source timestamps overlap the selected main-track
  GOP time ranges.
- Can preserve source-compatible timebase when all sources match, or use a
  manually configured timebase.
- Can generate continuous PTS/DTS for reordered GOP output.

Raw export:

- Writes video-only H.264 or HEVC elementary streams.
- Uses FFmpeg bitstream filters to output Annex B.
- Does not carry audio, subtitles, container metadata, PTS, or DTS.

## Compatibility

The current scope is H.264 and HEVC distribution streams.

For reliable replacement, replacement GOPs should match the main delivery stream:

- Same codec family: H.264 to H.264, or HEVC to HEVC.
- Same resolution, profile, level, pixel format, and codec extradata.
- Compatible timebase and intended frame-rate behavior for MP4 output.
- GOP cuts must start on key packets detected by the indexer.
- Replacement clips should cover the same display frame range as the original
  GOPs they replace.

Unsupported editing targets such as ProRes are rejected intentionally. GOP
Composer is not a general-purpose NLE and does not perform arbitrary frame-level
editing.

## Build From Source

Always use release builds for this Rust project:

```powershell
cargo build --release
```

Run:

```powershell
.\target\release\gop-composer.exe
```

### FFmpeg

GOP Composer links against shared FFmpeg libraries. On Windows, the build script
first looks for the project-local custom FFmpeg build:

```text
vendor\ffmpeg-gopcomposer-win64-msvc
```

Build it with:

```powershell
.\scripts\build-ffmpeg-windows-msvc.ps1 -Clean
```

Alternatively, point `FFMPEG_DIR` to an existing shared FFmpeg install:

```powershell
$env:FFMPEG_DIR = "D:\path\to\ffmpeg-shared"
```

The directory must contain `include`, `lib`, and `bin` on Windows. Runtime DLLs
are copied from `bin` into `target\release`.

The custom Windows FFmpeg build is intentionally narrow:

- Libraries: `avcodec`, `avformat`, `avutil`, `swscale`
- Demuxers: Matroska/MKV, MOV/MP4, MPEG-TS, raw H.264, raw HEVC
- Muxers: MP4/MOV
- Decoders: H.264 and HEVC
- Hardware decode: H.264/HEVC through D3D11VA, D3D12VA, and DXVA2
- Bitstream filters: `h264_mp4toannexb`, `hevc_mp4toannexb`, `aac_adtstoasc`,
  `extract_extradata`

`swresample` is not required because GOP Composer copies audio packets and does
not decode, resample, or encode audio. `swscale` is still used by the CPU RGBA
fallback preview path.

## CLI Utilities

The GUI is the primary interface, but a few CLI commands are available for
inspection and smoke testing:

```powershell
.\target\release\gop-composer.exe --probe "input.mkv"
.\target\release\gop-composer.exe --decode-window "input.mkv" 250 16
.\target\release\gop-composer.exe --export-gops "input.mkv" "2,5" mp4 "output.mp4"
.\target\release\gop-composer.exe --export-gops "input.mkv" "2-3" raw "output.h264"
.\target\release\gop-composer.exe --export-gops-ordered "input.mkv" "2,1,2" mp4 "output.mp4" --generate-pts
.\target\release\gop-composer.exe --timecode-map "main.mp4" "replacement_tc.mp4"
.\scripts\smoke-release.ps1
```

## Tests

```powershell
cargo test --release --locked
cargo clippy --release --locked --all-targets -- -D warnings
```

Small synthetic H.264, HEVC Main 10 and AAC samples are included in
[`tests/fixtures`](tests/fixtures). They contain only generated test patterns and
tones. See [testing instructions](docs/TESTING.md) for regeneration and full
packet/frame validation, and [release packaging](docs/RELEASING.md) to reproduce
the Windows archive.

## Keyboard Shortcuts

| Shortcut | Action |
| --- | --- |
| Left / Right | Step preview by one frame |
| Up / Down | Move between GOPs on the current timeline track |
| Delete | Delete selected timeline GOPs |
| Ctrl+X / Ctrl+C / Ctrl+V | Cut, copy, and paste selected GOPs |
| Ctrl+Z / Ctrl+Y | Undo and redo timeline edits |
| Space | Play or pause preview playback |

## Project Layout

```text
src/app/       Application state and editing workflow
  export.rs    Export planning, options and background work
  timeline.rs  GOP drag and move operations
src/media/     FFmpeg-facing media operations
src/preview/   Preview worker and GPU upload path
src/ui/        egui panels, styling, i18n, and window chrome
native/        C bridge code for FFmpeg and preview interop
scripts/       Build and smoke-test helpers
tests/         CLI regression tests and synthetic media fixtures
docs/          Testing, packaging and project notes
```

## Platform Status

Windows is the most developed and tested path today. The project is structured so
that platform-specific preview and window integration can be extended with
conditional compilation for macOS or Linux later.

## Limits

- Editing is GOP-boundary based, not arbitrary frame-boundary based.
- GOP Composer does not transcode replacement clips for you.
- MP4 export currently targets primary video and optional audio.
- Subtitle stream muxing is not implemented yet.
- Audio from replacement files is intentionally conservative. The main use case
  is video GOP replacement against a compatible delivery stream.
- Exported files should be validated with your own player, FFmpeg/ffprobe, and
  delivery QC tooling before production use.

## License

GOP Composer is distributed under the [MIT License](LICENSE).
