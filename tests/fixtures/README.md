# Synthetic media fixtures

All pictures and audio in this directory are generated from FFmpeg's `testsrc2`
and `sine` filters. There are no recordings, personal data, customer assets, or
third-party video clips. The fixtures are covered by the repository's MIT license.

| File | Video | Frames / GOPs | Audio | Timecode |
| --- | --- | --- | --- | --- |
| h264-main.mp4 | H.264, 160x96, 24 fps | 72 / 6 | AAC, 440 Hz tone | 01:00:00:00 |
| h264-replacement.mp4 | H.264, hue-shifted test pattern | 24 / 2 | None | 01:00:01:00 |
| h264-audio.mkv | Remux of h264-main.mp4 | 72 / 6 | AAC | Not used |
| hevc-main10.mp4 | HEVC Main 10, 160x96, 24 fps | 48 / 4 | None | None |

All GOPs are closed, 12 frames long, with B-frames enabled. The replacement maps
to main GOPs 3 and 4. `manifest.json` records the encoder version and SHA-256 hashes
of the checked-in files. Different FFmpeg/encoder versions may generate different
bytes; regression tests check media behavior rather than requiring a specific
encoder build.

Regenerate with a full FFmpeg build containing libx264, libx265 and lavfi:

```powershell
./scripts/generate-fixtures.ps1
```

The application's minimal FFmpeg DLL bundle is sufficient to run the Rust tests;
it does not include encoders or the FFmpeg CLI needed for regeneration.
