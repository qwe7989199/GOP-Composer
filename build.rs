use std::{
    env, fs,
    path::{Path, PathBuf},
};

const DEFAULT_WINDOWS_VENDOR_FFMPEG_DIR: &str = r"vendor\ffmpeg-gopcomposer-win64-msvc";
const DEFAULT_MACOS_FFMPEG_DIR: &str = "/opt/homebrew/opt/ffmpeg";
const DEFAULT_LINUX_FFMPEG_DIR: &str = "/usr";

fn main() {
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_else(|_| env::consts::OS.to_owned());
    let ffmpeg_dir =
        PathBuf::from(env::var("FFMPEG_DIR").unwrap_or_else(|_| default_ffmpeg_dir(&target_os)));
    let include_dir = ffmpeg_dir.join("include");
    let lib_dir = ffmpeg_dir.join("lib");
    let bin_dir = ffmpeg_dir.join("bin");

    println!("cargo:rerun-if-env-changed=FFMPEG_DIR");
    println!("cargo:rerun-if-changed=native/ffmpeg_bridge.h");
    println!("cargo:rerun-if-changed=native/file_io.h");
    println!("cargo:rerun-if-changed=native/file_io.c");
    println!("cargo:rerun-if-changed=native/ffmpeg_probe.c");
    println!("cargo:rerun-if-changed=native/ffmpeg_preview.c");
    println!("cargo:rerun-if-changed=native/gl_preview.c");
    println!("cargo:rerun-if-changed=native/ffmpeg_export.c");
    println!("cargo:rerun-if-changed=native/ffmpeg_export_segments.c");
    println!("cargo:rerun-if-changed=icon.ico");

    if !include_dir.exists() || !lib_dir.exists() || (target_os == "windows" && !bin_dir.exists()) {
        panic!(
            "FFmpeg directory is incomplete: {}. Expected include/ and lib/{}.",
            ffmpeg_dir.display(),
            if target_os == "windows" {
                ", plus bin/ on Windows"
            } else {
                ""
            }
        );
    }

    let mut bridge = cc::Build::new();
    bridge
        .file("native/file_io.c")
        .file("native/ffmpeg_probe.c")
        .file("native/ffmpeg_preview.c")
        .file("native/gl_preview.c")
        .file("native/ffmpeg_export.c")
        .file("native/ffmpeg_export_segments.c")
        .include(&include_dir)
        .warnings(false);
    match target_os.as_str() {
        "windows" => {
            bridge.define("GOPCOMPOSER_TARGET_WINDOWS", None);
        }
        "macos" => {
            bridge.define("GOPCOMPOSER_TARGET_MACOS", None);
        }
        "linux" => {
            bridge.define("GOPCOMPOSER_TARGET_LINUX", None);
        }
        _ => {}
    }
    bridge.compile("gopcomposer_ffmpeg_probe");

    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    for lib in ["avformat", "avcodec", "avutil", "swscale"] {
        println!("cargo:rustc-link-lib=dylib={lib}");
    }

    if target_os == "windows" {
        embed_windows_icon();
        for lib in ["d3d11", "dxgi", "dxguid", "opengl32", "user32"] {
            println!("cargo:rustc-link-lib=dylib={lib}");
        }
        copy_runtime_dlls(&bin_dir);
    }
}

#[cfg(target_os = "windows")]
fn embed_windows_icon() {
    let mut resource = winresource::WindowsResource::new();
    resource.set_icon("icon.ico");
    if let Err(error) = resource.compile() {
        panic!("failed to embed Windows icon: {error}");
    }
}

#[cfg(not(target_os = "windows"))]
fn embed_windows_icon() {}

fn default_ffmpeg_dir(target_os: &str) -> String {
    match target_os {
        "windows" => {
            let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_owned());
            let vendor_dir = PathBuf::from(manifest_dir).join(DEFAULT_WINDOWS_VENDOR_FFMPEG_DIR);
            vendor_dir.to_string_lossy().into_owned()
        }
        "macos" => DEFAULT_MACOS_FFMPEG_DIR.to_owned(),
        "linux" => DEFAULT_LINUX_FFMPEG_DIR.to_owned(),
        _ => DEFAULT_LINUX_FFMPEG_DIR.to_owned(),
    }
}

fn copy_runtime_dlls(bin_dir: &Path) {
    const KEEP_DLL_PREFIXES: &[&str] = &[
        "avformat-",
        "avcodec-",
        "avutil-",
        "swscale-",
        "swresample-",
    ];
    const FFMPEG_DLL_PREFIXES: &[&str] = &[
        "avformat-",
        "avcodec-",
        "avutil-",
        "swscale-",
        "swresample-",
        "avfilter-",
        "avdevice-",
        "postproc-",
    ];

    let Ok(manifest_dir) = env::var("CARGO_MANIFEST_DIR") else {
        return;
    };
    let profile = env::var("PROFILE").unwrap_or_else(|_| "release".to_owned());
    let target_dir = PathBuf::from(manifest_dir).join("target").join(profile);
    let _ = fs::create_dir_all(&target_dir);
    remove_ffmpeg_dlls(&target_dir, FFMPEG_DLL_PREFIXES);

    let Ok(entries) = fs::read_dir(bin_dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("dll"))
            && path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    KEEP_DLL_PREFIXES
                        .iter()
                        .any(|prefix| name.starts_with(prefix))
                })
        {
            let target = target_dir.join(path.file_name().expect("dll file name"));
            let _ = fs::copy(&path, target);
        }
    }
}

fn remove_ffmpeg_dlls(target_dir: &Path, ffmpeg_prefixes: &[&str]) {
    let Ok(entries) = fs::read_dir(target_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let is_ffmpeg = ffmpeg_prefixes
            .iter()
            .any(|prefix| name.starts_with(prefix));
        if is_ffmpeg {
            let _ = fs::remove_file(path);
        }
    }
}
