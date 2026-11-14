# Windows x64 release packaging

Prerequisites: stable Rust with the `x86_64-pc-windows-msvc` host, Visual Studio
C++ build tools and x64 redistributable files, Python 3.11+, and the custom shared
FFmpeg build. All Rust commands use release mode. The archive targets Windows 10/11 x64.

```powershell
cargo fmt --check
cargo test --release --locked
cargo clippy --release --locked --all-targets -- -D warnings
./scripts/smoke-release.ps1
python ./scripts/package-release.py
```

The packaging script verifies the executable version/architecture, compares each
runtime DLL to the selected FFmpeg install, and checks the FFmpeg source revision
and configure patch against its build manifest. It produces three files under
`target/dist/`:

- `GOP-Composer-v<version>-windows-x64.zip`: executable, four FFmpeg DLLs, official
  x64 `vcruntime140.dll`, documentation, synthetic samples and dependency licenses.
- `FFmpeg-<commit>-gop-composer-source.tar.gz`: matching LGPL source, configure
  portability patch already applied, build flags and the build script.
- `SHA256SUMS.txt`: hashes for both archives.

No private sample paths, PDB files, development SDKs or credentials are packaged.
The script uses Cargo's offline dependency metadata and downloads standard license
texts from the pinned SPDX license-list-data v3.27.0 tag where crates omit them.
Use `--binary-dir`, `--ffmpeg-dir`, `--ffmpeg-source`, `--vc-runtime` and
`--output-dir` to override paths.

The v0.1.0 FFmpeg DLLs were built from
`c29d710cd5d0f80bbb56f7ec1f35d5fb7ed44d05` with the flags in the archive's
`FFMPEG-BUILD.txt`. To rebuild that version with the repository script, prepare
the checkout first (the script's default `master` is for development):

```powershell
git clone https://github.com/FFmpeg/FFmpeg.git vendor/ffmpeg-src
git -C vendor/ffmpeg-src checkout c29d710cd5d0f80bbb56f7ec1f35d5fb7ed44d05
./scripts/build-ffmpeg-windows-msvc.ps1 -NoUpdate -Clean
```

This needs MSYS2 with `make`, `diffutils`, `pkg-config` and `nasm`, plus Visual
Studio C++ tools. The script applies only a compiler-identification portability
patch. It does not enable GPL or nonfree components. The corresponding source
archive also allows configuring the included patched tree directly with the
recorded flags in an MSVC-enabled MSYS2 shell.

Extract the ZIP into a fresh directory and run `scripts/smoke-release.ps1
-SkipBuild -Executable <unpacked-exe>` before publishing. Also check GUI startup
and exercise preview on representative hardware. Upload all three artifacts to a
GitHub Release pointing to the reviewed source commit; publish the matching tag.
