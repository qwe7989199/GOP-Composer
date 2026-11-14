# Open Source Checklist

GOP Composer is public. This document tracks follow-up work after the first release.

## Completed for the first public release

- MIT license and repository metadata in `Cargo.toml`.
- `target/` and `vendor/` excluded from git.
- Synthetic test fixtures with documented provenance and hashes.
- Automated export, preview, timeline and CLI regression tests.
- Windows x64 packaging with FFmpeg DLLs, dependency notices and matching source.
- Testing and release instructions in [TESTING.md](TESTING.md) and
  [RELEASING.md](RELEASING.md).

## Good first public issues

- Subtitle stream muxing.
- macOS preview and hardware decode path.
- Linux preview and hardware decode path.
- More detailed stream compatibility reporting.
- Export report with source GOP mapping and timestamp mode.
- Broader audio synchronization, open-GOP and variable-frame-rate fixtures.
- CI across supported operating systems and representative GPU hardware.

## Release artifact notes

For Windows releases, include:

- `gop-composer.exe`
- `avcodec-*.dll`
- `avformat-*.dll`
- `avutil-*.dll`
- `swscale-*.dll`
- `vcruntime140.dll` from the official Visual Studio x64 Redist directory
- Dependency licenses and notices

Provide the matching FFmpeg source as a separate release artifact. Keep build
trees and development SDKs out of the application archive.

## Documentation still needed

- A user guide for the subtitle-fix replacement workflow.
- A compatibility guide explaining codec parameters, extradata, timebase, and
  GOP boundaries.
- A troubleshooting page for mux errors and preview hardware decode failures.
- A screenshot or short demo GIF made using the synthetic fixtures.
