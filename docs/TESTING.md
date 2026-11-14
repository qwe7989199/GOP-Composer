# Tests and synthetic media

Use release mode for every Rust build, test and lint invocation:

```powershell
cargo fmt --check
cargo test --release --locked
cargo clippy --release --locked --all-targets -- -D warnings
./scripts/smoke-release.ps1
```

The Rust tests only need the FFmpeg development/runtime installation described
in the README. They cover GOP indexing, H.264 and HEVC Main 10 preview, raw
round-trips, B-frame timestamps, compatible replacement, Unicode paths, input
overwrite protection (including hard links), failed-export preservation, timeline
dragging, and CLI exit codes/timecode mapping. Each test owns its temporary output.

The smoke script additionally needs full `ffmpeg` and `ffprobe` executables on
PATH. It checks the committed fixture hashes, copied AAC audio, exact compressed
video packet hashes, decoded picture hashes and continuous DTS for reordered and
repeated GOPs, with generated PTS both enabled and disabled. It fails immediately
when any native command fails. Use `-Ffmpeg` and `-Ffprobe` to select explicit tools.

To validate an unpacked release without rebuilding:

```powershell
./scripts/smoke-release.ps1 -SkipBuild -Executable 'D:/unpacked/gop-composer.exe'
```

See [fixtures](../tests/fixtures/README.md) for generation commands and provenance.
No private media is needed. Hardware availability varies by machine; an unsupported
hardware format must fall back to software decoding. GPU-specific presentation
still benefits from manual testing on representative Windows hardware.
