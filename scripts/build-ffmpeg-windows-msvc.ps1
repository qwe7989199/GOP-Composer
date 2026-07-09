param(
    [string]$SourceDir = "",
    [string]$BuildDir = "",
    [string]$Prefix = "",
    [string]$Repo = "https://github.com/FFmpeg/FFmpeg.git",
    [string]$Ref = "master",
    [switch]$NoUpdate,
    [switch]$Clean,
    [switch]$BuildPrograms
)

$ErrorActionPreference = "Stop"

$projectRoot = Split-Path -Parent $PSScriptRoot
if ([string]::IsNullOrWhiteSpace($SourceDir)) {
    $SourceDir = Join-Path $projectRoot "vendor\ffmpeg-src"
}
if ([string]::IsNullOrWhiteSpace($BuildDir)) {
    $BuildDir = Join-Path $projectRoot "vendor\ffmpeg-build-msvc"
}
if ([string]::IsNullOrWhiteSpace($Prefix)) {
    $Prefix = Join-Path $projectRoot "vendor\ffmpeg-gopcomposer-win64-msvc"
}

function Resolve-VisualStudioVcVars {
    $vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
    if (Test-Path $vswhere) {
        $install = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
        if (![string]::IsNullOrWhiteSpace($install)) {
            $candidate = Join-Path $install "VC\Auxiliary\Build\vcvars64.bat"
            if (Test-Path $candidate) {
                return $candidate
            }
        }
    }

    $fallback = "C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Auxiliary\Build\vcvars64.bat"
    if (Test-Path $fallback) {
        return $fallback
    }
    throw "vcvars64.bat not found. Install Visual Studio 2022 C++ build tools."
}

function Resolve-MsysBash {
    foreach ($candidate in @("C:\msys64\usr\bin\bash.exe", "C:\msys64\msys2_shell.cmd")) {
        if (Test-Path $candidate) {
            return $candidate
        }
    }
    $cmd = Get-Command bash.exe -ErrorAction SilentlyContinue
    if ($cmd) {
        return $cmd.Source
    }
    throw "MSYS2 bash not found. Install MSYS2 with make, diffutils, pkg-config, and yasm/nasm."
}

function Convert-ToMsysPath([string]$Path) {
    $bash = Resolve-MsysBash
    $resolved = [System.IO.Path]::GetFullPath($Path)
    $converted = & $bash -lc "cygpath -u '$($resolved -replace '\\', '\\')'"
    if ([string]::IsNullOrWhiteSpace($converted)) {
        throw "Failed to convert path for MSYS2: $resolved"
    }
    return $converted.Trim()
}

$vcvars = Resolve-VisualStudioVcVars
$bash = Resolve-MsysBash

New-Item -ItemType Directory -Force (Split-Path -Parent $SourceDir) | Out-Null

if (!(Test-Path (Join-Path $SourceDir ".git"))) {
    if (Test-Path $SourceDir) {
        if ($Clean) {
            Remove-Item -LiteralPath $SourceDir -Recurse -Force
        } else {
            throw "SourceDir exists but is not a git checkout: $SourceDir"
        }
    }
    git clone --depth 1 --branch $Ref $Repo $SourceDir
} elseif (!$NoUpdate) {
    Push-Location $SourceDir
    try {
        git fetch --depth 1 origin $Ref
        git checkout -B $Ref FETCH_HEAD
    } finally {
        Pop-Location
    }
}

$configurePath = Join-Path $SourceDir "configure"
$configureText = [System.IO.File]::ReadAllText($configurePath)
$patchedConfigureText = $configureText.
    Replace("grep -q ^Microsoft", "grep -q Microsoft").
    Replace("grep ^Microsoft", "grep Microsoft")
if ($patchedConfigureText -ne $configureText) {
    [System.IO.File]::WriteAllText($configurePath, $patchedConfigureText, [System.Text.UTF8Encoding]::new($false))
}

if ($Clean) {
    Remove-Item -LiteralPath $BuildDir -Recurse -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $Prefix -Recurse -Force -ErrorAction SilentlyContinue
}
New-Item -ItemType Directory -Force $BuildDir | Out-Null
New-Item -ItemType Directory -Force $Prefix | Out-Null

$sourceUnix = Convert-ToMsysPath $SourceDir
$buildUnix = Convert-ToMsysPath $BuildDir
$prefixUnix = Convert-ToMsysPath $Prefix

$programFlags = if ($BuildPrograms) { "--enable-ffmpeg --enable-ffprobe" } else { "--disable-programs" }

$configureFlags = @(
    "--prefix=$prefixUnix",
    "--toolchain=msvc",
    "--target-os=win64",
    "--arch=x86_64",
    "--enable-shared",
    "--disable-static",
    "--disable-debug",
    "--disable-doc",
    "--disable-autodetect",
    "--disable-everything",
    "--disable-avdevice",
    "--disable-avfilter",
    "--disable-swresample",
    "--enable-avcodec",
    "--enable-avformat",
    "--enable-avutil",
    "--enable-swscale",
    "--enable-d3d11va",
    "--enable-d3d12va",
    "--enable-dxva2",
    "--enable-protocol=file",
    "--enable-demuxer=matroska,mov,mpegts,h264,hevc",
    "--enable-muxer=mp4",
    "--enable-parser=h264,hevc,aac,ac3",
    "--enable-decoder=h264,hevc",
    "--enable-bsf=h264_mp4toannexb,hevc_mp4toannexb,aac_adtstoasc,extract_extradata",
    "--enable-hwaccel=h264_d3d11va,h264_d3d12va,h264_dxva2,hevc_d3d11va,hevc_d3d12va,hevc_dxva2",
    $programFlags
)

$commit = git -C $SourceDir rev-parse HEAD
Write-Host "FFmpeg source: $SourceDir"
Write-Host "FFmpeg commit: $commit"
Write-Host "Install prefix: $Prefix"

$bashScript = @"
set -euo pipefail
export PATH="`$PATH:/usr/bin"
cd "$buildUnix"
"$sourceUnix/configure" $($configureFlags -join " ")
sed -i 's|^#define CC_IDENT .*|#define CC_IDENT "msvc"|' config.h
make -j`$(nproc)
make install
cp "$prefixUnix/bin/"*.lib "$prefixUnix/lib/"
cat > "$prefixUnix/GOPCOMPOSER_FFMPEG_BUILD.txt" <<'EOF'
repo=$Repo
ref=$Ref
commit=$commit
toolchain=msvc
flags=$($configureFlags -join " ")
EOF
"@

$tempScript = Join-Path $BuildDir "build-ffmpeg-msvc.sh"
$utf8NoBom = [System.Text.UTF8Encoding]::new($false)
[System.IO.File]::WriteAllText($tempScript, $bashScript, $utf8NoBom)
$tempScriptUnix = Convert-ToMsysPath $tempScript

cmd /c "call `"$vcvars`" >nul && `"$bash`" --noprofile --norc `"$tempScriptUnix`""
if ($LASTEXITCODE -ne 0) {
    throw "FFmpeg build failed with exit code $LASTEXITCODE"
}

Write-Host "FFmpeg build completed: $Prefix"
