use std::{fs, path::PathBuf};

use eframe::egui;

pub(crate) fn install_system_fonts(ctx: &egui::Context) {
    let Some((name, bytes)) = load_cjk_font() else {
        eprintln!("No system CJK font found; falling back to egui default fonts");
        return;
    };

    let mut fonts = egui::FontDefinitions::default();
    fonts
        .font_data
        .insert(name.clone(), egui::FontData::from_owned(bytes).into());
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .insert(0, name.clone());
    }
    ctx.set_fonts(fonts);
}

fn load_cjk_font() -> Option<(String, Vec<u8>)> {
    system_cjk_font_paths().into_iter().find_map(|path| {
        fs::read(&path).ok().map(|bytes| {
            (
                path.file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or("system-cjk")
                    .to_owned(),
                bytes,
            )
        })
    })
}

#[cfg(target_os = "windows")]
fn system_cjk_font_paths() -> Vec<PathBuf> {
    let font_dir = std::env::var_os("WINDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
        .join("Fonts");
    [
        "msyh.ttc",
        "msyhbd.ttc",
        "msyhl.ttc",
        "simsun.ttc",
        "simsunb.ttf",
        "simhei.ttf",
    ]
    .into_iter()
    .map(|name| font_dir.join(name))
    .collect()
}

#[cfg(target_os = "macos")]
fn system_cjk_font_paths() -> Vec<PathBuf> {
    [
        "/System/Library/Fonts/PingFang.ttc",
        "/System/Library/Fonts/STHeiti Light.ttc",
        "/System/Library/Fonts/STHeiti Medium.ttc",
        "/System/Library/Fonts/Hiragino Sans GB.ttc",
    ]
    .into_iter()
    .map(PathBuf::from)
    .collect()
}

#[cfg(target_os = "linux")]
fn system_cjk_font_paths() -> Vec<PathBuf> {
    [
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
        "/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf",
    ]
    .into_iter()
    .map(PathBuf::from)
    .collect()
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
fn system_cjk_font_paths() -> Vec<PathBuf> {
    Vec::new()
}
