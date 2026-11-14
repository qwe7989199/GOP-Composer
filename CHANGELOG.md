# Changelog

## 0.1.0

First public Windows x64 release.

- Prevent exports from overwriting an input file, including hard-link aliases.
- Write exports to a sibling temporary file and publish only after success, so
  an incompatible later segment cannot destroy an existing destination.
- Propagate MP4 close/flush failures instead of reporting a successful export.
- Remove B-frame timestamp gaps between reordered GOPs.
- Fall back to software decoding when a hardware pixel format cannot initialize,
  including HEVC Main 10 on unsupported hardware.
- Correct dragging a GOP to the end of its own track.
- Split application export and timeline operations into dedicated modules.
- Place FFmpeg DLLs in the actual Cargo output directory, including custom
  target directories and explicit target triples.
- Add CLI help/version and reject unknown commands with exit code 2.
- Add small synthetic H.264/AAC/MKV/HEVC fixtures and automated regression tests.
