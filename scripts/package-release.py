"""Package a built Windows x64 release and its corresponding FFmpeg source.

Uses Python 3.11+ standard library only. Build/test with Cargo --release first.
"""

import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import re
import subprocess
import tarfile
import tomllib
import urllib.request
import zipfile


ROOT = Path(__file__).resolve().parent.parent


def run(*args, cwd=ROOT):
    return subprocess.check_output(args, cwd=cwd)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary-dir', type=Path, default=ROOT / 'target/release')
    parser.add_argument('--ffmpeg-dir', type=Path, default=ROOT / 'vendor/ffmpeg-gopcomposer-win64-msvc')
    parser.add_argument('--ffmpeg-source', type=Path, default=ROOT / 'vendor/ffmpeg-src')
    parser.add_argument('--vc-runtime', type=Path, help='Official x64 vcruntime140.dll from Visual Studio Redist')
    parser.add_argument('--output-dir', type=Path, default=ROOT / 'target/dist')
    args = parser.parse_args()
    version = tomllib.loads((ROOT / 'Cargo.toml').read_text())['package']['version']
    executable = args.binary_dir / 'gop-composer.exe'
    data = executable.read_bytes()
    pe_offset = int.from_bytes(data[0x3c:0x40], 'little')
    if data[pe_offset:pe_offset + 6] != b'PE\0\0\x64\x86':
        raise RuntimeError('Release executable must be a Windows x64 PE image')
    if run(str(executable.resolve()), '--version').decode().strip() != f'GOP Composer {version}':
        raise RuntimeError('Executable version differs from Cargo.toml')

    build_info = dict(line.split('=', 1) for line in
                      (args.ffmpeg_dir / 'GOPCOMPOSER_FFMPEG_BUILD.txt').read_text().splitlines())
    commit = build_info['commit']
    if run('git', 'rev-parse', 'HEAD', cwd=args.ffmpeg_source).decode().strip() != commit:
        raise RuntimeError('FFmpeg checkout differs from the DLL build manifest')
    # Only the build script's documented MSVC compiler-identification patch is allowed.
    original_configure = run('git', 'show', 'HEAD:configure', cwd=args.ffmpeg_source)
    expected_configure = original_configure.replace(b'grep -q ^Microsoft', b'grep -q Microsoft').replace(
        b'grep ^Microsoft', b'grep Microsoft')
    if (args.ffmpeg_source / 'configure').read_bytes().replace(b'\r\n', b'\n') != expected_configure:
        raise RuntimeError('Unexpected local FFmpeg configure changes')
    changed = run('git', 'diff', '--name-only', 'HEAD', cwd=args.ffmpeg_source).decode().splitlines()
    if any(name != 'configure' for name in changed):
        raise RuntimeError('Unexpected local FFmpeg source changes')

    runtime = args.vc_runtime
    if runtime is None:
        vswhere = Path(os.environ['ProgramFiles(x86)']) / 'Microsoft Visual Studio/Installer/vswhere.exe'
        install = Path(run(str(vswhere), '-latest', '-products', '*', '-requires',
                           'Microsoft.VisualStudio.Component.VC.Tools.x86.x64',
                           '-property', 'installationPath').decode().strip())
        candidates = sorted((install / 'VC/Redist/MSVC').glob('*/x64/Microsoft.VC*.CRT/vcruntime140.dll'))
        if not candidates:
            raise RuntimeError('Cannot find the official Visual C++ x64 runtime')
        runtime = candidates[-1]

    metadata = json.loads(run('cargo', 'metadata', '--locked', '--offline', '--format-version', '1',
                             '--filter-platform', 'x86_64-pc-windows-msvc'))
    args.output_dir.mkdir(parents=True, exist_ok=True)
    archive = args.output_dir / f'GOP-Composer-v{version}-windows-x64.zip'
    source_archive = args.output_dir / f'FFmpeg-{commit[:7]}-gop-composer-source.tar.gz'
    sanitized_build_info = {key: re.sub(r'--prefix=\S+', '--prefix=<install-prefix>', value)
                            for key, value in build_info.items()}
    sanitized_info = '\n'.join(f'{key}={value}' for key, value in sanitized_build_info.items()) + '\n'
    with zipfile.ZipFile(archive, 'w', zipfile.ZIP_DEFLATED, compresslevel=9, strict_timestamps=False) as out:
        out.write(executable, executable.name)
        out.write(runtime, 'vcruntime140.dll')
        for prefix in ['avcodec-', 'avformat-', 'avutil-', 'swscale-']:
            dlls = list(args.ffmpeg_dir.joinpath('bin').glob(f'{prefix}*.dll'))
            if len(dlls) != 1:
                raise RuntimeError(f'Expected exactly one {prefix} DLL')
            dll = dlls[0]
            if dll.read_bytes() != (args.binary_dir / dll.name).read_bytes():
                raise RuntimeError(f'Build/runtime DLL mismatch: {dll.name}')
            out.write(dll, dll.name)
        for name in ['LICENSE', 'README.md', 'README.zh-CN.md', 'CHANGELOG.md']:
            out.write(ROOT / name, name)
        for path in sorted((ROOT / 'docs').glob('*.md')):
            out.write(path, f'docs/{path.name}')
        for path in sorted((ROOT / 'tests/fixtures').iterdir()):
            if path.is_file():
                out.write(path, f'tests/fixtures/{path.name}')
        out.writestr('FFMPEG-BUILD.txt', sanitized_info)
        out.write(args.ffmpeg_source / 'COPYING.LGPLv2.1', 'licenses/FFmpeg-LGPL-2.1.txt')
        notices = [
            '# Third-party notices', '',
            f'FFmpeg source commit: {commit}',
            'FFmpeg is dynamically linked under LGPL-2.1-or-later. Matching source is',
            f'provided alongside this ZIP as {source_archive.name}.',
            'The configure portability patch and build instructions are included there.',
            '', 'vcruntime140.dll: Microsoft Visual C++ Runtime, Copyright Microsoft Corporation.',
            'Redistributed from the Visual Studio x64 Redist directory under its redistributable terms.',
            'https://visualstudio.microsoft.com/license-terms/', '',
            '## Rust dependencies (including build/optional dependencies)', '',
        ]
        missing = []
        for package in sorted(metadata['packages'], key=lambda p: (p['name'], p['version'])):
            if package['name'] == 'gop-composer':
                continue
            label = f"{package['name']}-{package['version']}"
            notices.append(f"- {label}: {package['license'] or 'see license file'}; {package['repository'] or ''}")
            base = Path(package['manifest_path']).parent
            licenses = [p for p in base.rglob('*') if p.is_file() and
                        p.name.lower().startswith(('license', 'licence', 'copying', 'notice', 'ofl', 'ufl'))]
            for path in licenses:
                out.write(path, f'licenses/rust/{label}/{path.relative_to(base).as_posix()}')
            if not licenses:
                missing.append(label)
        # Some crates omit license text from their published package. Include the
        # standard license terms as well as the package attribution and repository.
        for license_id in ['Apache-2.0', 'MIT', 'BSL-1.0', 'CC0-1.0', 'OFL-1.1', 'Ubuntu-font-1.0']:
            url = f'https://raw.githubusercontent.com/spdx/license-list-data/v3.27.0/text/{license_id}.txt'
            with urllib.request.urlopen(url, timeout=30) as response:
                out.writestr(f'licenses/standard/{license_id}.txt', response.read())
        notices += ['', 'Packages without bundled license files (standard terms above apply):', ', '.join(missing), '']
        out.writestr('THIRD-PARTY-NOTICES.md', '\n'.join(notices))

    raw_archive = run('git', 'archive', '--format=tar', '--prefix=ffmpeg/', 'HEAD', cwd=args.ffmpeg_source)
    with tarfile.open(fileobj=io.BytesIO(raw_archive)) as source, tarfile.open(source_archive, 'w:gz') as out:
        for member in source:
            if member.name == 'ffmpeg/configure':
                member.size = len(expected_configure)
                out.addfile(member, io.BytesIO(expected_configure))
            else:
                out.addfile(member, source.extractfile(member) if member.isfile() else None)
        for name, content in {
            'GOPCOMPOSER_FFMPEG_BUILD.txt': sanitized_info.encode(),
            'build-ffmpeg-windows-msvc.ps1': (ROOT / 'scripts/build-ffmpeg-windows-msvc.ps1').read_bytes(),
            'README-GOP-COMPOSER.md': (
                'This is the corresponding FFmpeg source for the bundled shared DLLs.\n'
                'The MSVC compiler-identification patch is already applied to configure.\n'
                'GOPCOMPOSER_FFMPEG_BUILD.txt records the exact commit and configure flags.\n'
                'See docs/RELEASING.md in GOP-Composer for prerequisites and build instructions.\n'
            ).encode(),
        }.items():
            member = tarfile.TarInfo(f'ffmpeg/{name}')
            member.size = len(content)
            out.addfile(member, io.BytesIO(content))
    checksums = ''.join(f'{hashlib.sha256(p.read_bytes()).hexdigest()}  {p.name}\n'
                        for p in [archive, source_archive])
    (args.output_dir / 'SHA256SUMS.txt').write_text(checksums, encoding='utf-8')
    print(checksums, end='')
    print(f'Packaged {archive.stat().st_size:,} bytes; FFmpeg source {source_archive.stat().st_size:,} bytes')


if __name__ == '__main__':
    main()
