use eframe::egui;

use crate::media::ffmpeg::{ExportKind, GopInfo, MediaType, ProbeSummary, StreamInfo};

pub(crate) fn anchor_gop_for_frame(probe: &ProbeSummary, frame: i64) -> Option<&GopInfo> {
    probe
        .gops
        .iter()
        .find(|gop| frame >= gop.start_frame && frame <= gop.end_frame)
        .or_else(|| probe.gops.iter().rev().find(|gop| gop.start_frame <= frame))
        .or_else(|| probe.gops.first())
}

pub(crate) fn find_gop_index(probe: &ProbeSummary, frame: i64) -> Option<usize> {
    anchor_gop_for_frame(probe, frame).map(|gop| gop.index)
}

pub(crate) fn export_kind_label(kind: ExportKind) -> &'static str {
    match kind {
        ExportKind::Mp4 => "MP4",
        ExportKind::Raw => "raw",
    }
}

pub(crate) fn format_bytes(value: i64) -> String {
    let mut size = value.max(0) as f64;
    for unit in ["B", "KB", "MB", "GB"] {
        if size < 1024.0 || unit == "GB" {
            return if unit == "B" {
                format!("{} {unit}", size as i64)
            } else {
                format!("{size:.2} {unit}")
            };
        }
        size /= 1024.0;
    }
    format!("{value} B")
}

pub(crate) fn stream_details(stream: &StreamInfo) -> String {
    match stream.media_type {
        MediaType::Video => {
            if stream.width > 0 && stream.height > 0 {
                format!("{}x{}", stream.width, stream.height)
            } else {
                "-".to_owned()
            }
        }
        MediaType::Audio => {
            let rate = if stream.sample_rate > 0 {
                format!("{} Hz", stream.sample_rate)
            } else {
                "unknown Hz".to_owned()
            };
            let channels = if stream.channels > 0 {
                format!("{} ch", stream.channels)
            } else {
                "unknown ch".to_owned()
            };
            format!("{rate}, {channels}")
        }
        MediaType::Other(value) => format!("type={value}"),
        _ => "-".to_owned(),
    }
}

pub(crate) fn track_color(index: usize) -> egui::Color32 {
    const COLORS: [egui::Color32; 6] = [
        egui::Color32::from_rgb(132, 179, 232),
        egui::Color32::from_rgb(115, 199, 187),
        egui::Color32::from_rgb(244, 181, 126),
        egui::Color32::from_rgb(235, 143, 165),
        egui::Color32::from_rgb(166, 190, 224),
        egui::Color32::from_rgb(186, 166, 226),
    ];
    COLORS[index % COLORS.len()]
}

pub(crate) fn mix_color(a: egui::Color32, b: egui::Color32, amount_a: f32) -> egui::Color32 {
    let amount_a = amount_a.clamp(0.0, 1.0);
    let amount_b = 1.0 - amount_a;
    egui::Color32::from_rgb(
        (a.r() as f32 * amount_a + b.r() as f32 * amount_b).round() as u8,
        (a.g() as f32 * amount_a + b.g() as f32 * amount_b).round() as u8,
        (a.b() as f32 * amount_a + b.b() as f32 * amount_b).round() as u8,
    )
}

pub(crate) fn pastel_highlight(color: egui::Color32) -> egui::Color32 {
    let (hue, saturation, lightness) = rgb_to_hsl(color);
    hsl_to_rgb(hue, saturation * 0.28, (lightness + 0.2).clamp(0.88, 0.94))
}

fn rgb_to_hsl(color: egui::Color32) -> (f32, f32, f32) {
    let red = color.r() as f32 / 255.0;
    let green = color.g() as f32 / 255.0;
    let blue = color.b() as f32 / 255.0;
    let max = red.max(green).max(blue);
    let min = red.min(green).min(blue);
    let lightness = (max + min) * 0.5;
    if (max - min).abs() <= f32::EPSILON {
        return (0.0, 0.0, lightness);
    }

    let delta = max - min;
    let saturation = if lightness > 0.5 {
        delta / (2.0 - max - min)
    } else {
        delta / (max + min)
    };
    let hue = if (max - red).abs() <= f32::EPSILON {
        (green - blue) / delta + if green < blue { 6.0 } else { 0.0 }
    } else if (max - green).abs() <= f32::EPSILON {
        (blue - red) / delta + 2.0
    } else {
        (red - green) / delta + 4.0
    } / 6.0;
    (hue, saturation, lightness)
}

fn hsl_to_rgb(hue: f32, saturation: f32, lightness: f32) -> egui::Color32 {
    if saturation <= f32::EPSILON {
        let value = normalized_to_u8(lightness);
        return egui::Color32::from_rgb(value, value, value);
    }

    let q = if lightness < 0.5 {
        lightness * (1.0 + saturation)
    } else {
        lightness + saturation - lightness * saturation
    };
    let p = 2.0 * lightness - q;
    egui::Color32::from_rgb(
        normalized_to_u8(hue_to_rgb(p, q, hue + 1.0 / 3.0)),
        normalized_to_u8(hue_to_rgb(p, q, hue)),
        normalized_to_u8(hue_to_rgb(p, q, hue - 1.0 / 3.0)),
    )
}

fn hue_to_rgb(p: f32, q: f32, mut t: f32) -> f32 {
    if t < 0.0 {
        t += 1.0;
    }
    if t > 1.0 {
        t -= 1.0;
    }
    if t < 1.0 / 6.0 {
        p + (q - p) * 6.0 * t
    } else if t < 0.5 {
        q
    } else if t < 2.0 / 3.0 {
        p + (q - p) * (2.0 / 3.0 - t) * 6.0
    } else {
        p
    }
}

fn normalized_to_u8(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

pub(crate) fn choose_tick_step(pixels_per_frame: f32) -> i64 {
    for step in [1, 5, 10, 25, 50, 100, 250, 500, 1000, 2500, 5000] {
        if step as f32 * pixels_per_frame >= 80.0 {
            return step;
        }
    }
    10_000
}

pub(crate) fn truncate_middle(text: &str, max_chars: usize) -> String {
    let char_count = text.chars().count();
    if char_count <= max_chars || max_chars < 5 {
        return text.to_owned();
    }
    let keep = (max_chars - 1) / 2;
    let start = text.chars().take(keep).collect::<String>();
    let end = text
        .chars()
        .rev()
        .take(max_chars - keep - 1)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<String>();
    format!("{start}...{end}")
}
