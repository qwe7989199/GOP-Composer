# Open Source Checklist

This checklist tracks what should be settled before making GOP Composer public.

## Required before public release

- Choose a license and add `LICENSE`.
- Add repository URL and license metadata to `Cargo.toml`.
- Confirm that no personal paths, private sample names, or local-only paths are
  documented as defaults.
- Keep `target/` and `vendor/` out of git.
- Decide whether to publish prebuilt FFmpeg DLLs, publish a separate binary
  release, or require users to run the FFmpeg build script.
- Add at least one screenshot or short demo GIF.
- Add a short disclaimer that exported files should be validated before
  production delivery.

## Good first public issues

- Subtitle stream muxing.
- macOS preview and hardware decode path.
- Linux preview and hardware decode path.
- More detailed stream compatibility reporting.
- Export report with source GOP mapping and timestamp mode.
- Automated mux regression tests with small synthetic H.264/HEVC samples.

## Release artifact notes

For Windows releases, include:

- `gop-composer.exe`
- `avcodec-*.dll`
- `avformat-*.dll`
- `avutil-*.dll`
- `swscale-*.dll`

Do not include FFmpeg source or build trees in the application archive.

## Documentation still needed

- A user guide for the subtitle-fix replacement workflow.
- A compatibility guide explaining codec parameters, extradata, timebase, and
  GOP boundaries.
- A troubleshooting page for mux errors and preview hardware decode failures.
