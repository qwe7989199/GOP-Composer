use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    time::Instant,
};

use crate::{
    media::ffmpeg::{self, ExportKind, ExportOptions, ExportSegment, PacketRange, ProbeSummary},
    util::{anchor_gop_for_frame, stream_details},
};

pub(crate) fn try_handle(args: &[String]) -> Option<eframe::Result<()>> {
    match args.first().map(String::as_str) {
        Some("--probe") => Some(handle_probe(args)),
        Some("--decode-frame") => Some(handle_decode_frame(args)),
        Some("--decode-window") => Some(handle_decode_window(args)),
        Some("--export-range") => Some(handle_export_range(args)),
        Some("--export-ranges") => Some(handle_export_ranges(args)),
        Some("--export-gops") => Some(handle_export_gops(args)),
        Some("--export-gops-ordered") => Some(handle_export_gops_ordered(args)),
        Some("--timecode-map") => Some(handle_timecode_map(args)),
        Some("--bench-preview-session") => Some(handle_preview_bench(args)),
        _ => None,
    }
}

fn handle_probe(args: &[String]) -> eframe::Result<()> {
    let Some(path) = args.get(1) else {
        eprintln!("Usage: gop-composer --probe <video>");
        std::process::exit(2);
    };
    match ffmpeg::probe_gops(&PathBuf::from(path)) {
        Ok(probe) => {
            print_probe_summary(&probe);
            Ok(())
        }
        Err(error) => {
            eprintln!("Probe failed: {error}");
            std::process::exit(1);
        }
    }
}

fn handle_decode_frame(args: &[String]) -> eframe::Result<()> {
    let (Some(path), Some(frame), Some(output)) = (args.get(1), args.get(2), args.get(3)) else {
        eprintln!("Usage: gop-composer --decode-frame <video> <frame> <out.rgba>");
        std::process::exit(2);
    };
    let frame = parse_i64(frame, "frame index");
    match ffmpeg::decode_rgba_frame(&PathBuf::from(path), frame) {
        Ok(image) => {
            if let Err(error) = fs::write(output, &image.pixels) {
                eprintln!("Failed to write output: {error}");
                std::process::exit(1);
            }
            println!(
                "decoded frame {}: {}x{} rgba bytes={} checksum={:016x}",
                frame,
                image.width,
                image.height,
                image.pixels.len(),
                rgba_checksum(&image.pixels)
            );
            Ok(())
        }
        Err(error) => {
            eprintln!("Decode failed: {error}");
            std::process::exit(1);
        }
    }
}

fn handle_decode_window(args: &[String]) -> eframe::Result<()> {
    let (Some(path), Some(start), Some(count)) = (args.get(1), args.get(2), args.get(3)) else {
        eprintln!("Usage: gop-composer --decode-window <video> <start-frame> <count>");
        std::process::exit(2);
    };
    let path = PathBuf::from(path);
    match decode_window_smoke(
        &path,
        parse_i64(start, "start frame"),
        parse_i32(count, "frame count"),
    ) {
        Ok(()) => Ok(()),
        Err(error) => {
            eprintln!("Decode window failed: {error}");
            std::process::exit(1);
        }
    }
}

fn handle_export_range(args: &[String]) -> eframe::Result<()> {
    let (Some(input), Some(start), Some(end), Some(kind), Some(output)) = (
        args.get(1),
        args.get(2),
        args.get(3),
        args.get(4),
        args.get(5),
    ) else {
        eprintln!(
            "Usage: gop-composer --export-range <input> <start-packet> <end-packet> <mp4|raw> <output>"
        );
        std::process::exit(2);
    };
    let started_at = Instant::now();
    let result = ffmpeg::export_video_range(
        &PathBuf::from(input),
        &PathBuf::from(output),
        parse_i64(start, "start packet"),
        parse_i64(end, "end packet"),
        parse_export_kind(kind),
    );
    print_export_result(result, started_at)
}

fn handle_export_ranges(args: &[String]) -> eframe::Result<()> {
    let (Some(input), Some(ranges), Some(kind), Some(output)) =
        (args.get(1), args.get(2), args.get(3), args.get(4))
    else {
        eprintln!(
            "Usage: gop-composer --export-ranges <input> <start-end,start-end> <mp4|raw> <output>"
        );
        std::process::exit(2);
    };
    let ranges = parse_packet_ranges(ranges);
    let started_at = Instant::now();
    let result = ffmpeg::export_video_ranges(
        &PathBuf::from(input),
        &PathBuf::from(output),
        &ranges,
        parse_export_kind(kind),
    );
    match result {
        Ok(summary) => {
            println!(
                "exported {} ranges, {} packets to {} ({} bytes reported) in {:.0}ms",
                ranges.len(),
                summary.packets_written,
                summary.output_path,
                summary.bytes_written,
                started_at.elapsed().as_secs_f64() * 1000.0
            );
            Ok(())
        }
        Err(error) => {
            eprintln!("Export failed: {error}");
            std::process::exit(1);
        }
    }
}

fn handle_export_gops(args: &[String]) -> eframe::Result<()> {
    let (Some(input), Some(gops), Some(kind), Some(output)) =
        (args.get(1), args.get(2), args.get(3), args.get(4))
    else {
        eprintln!("Usage: gop-composer --export-gops <input> <gop-list> <mp4|raw> <output>");
        eprintln!("Example gop-list: 2,5 or 2-5");
        std::process::exit(2);
    };
    let input_path = PathBuf::from(input);
    let started_at = Instant::now();
    let probe = match ffmpeg::probe_gops(&input_path) {
        Ok(probe) => probe,
        Err(error) => {
            eprintln!("Probe failed: {error}");
            std::process::exit(1);
        }
    };
    let gop_indices = parse_gop_selection(gops, probe.gops.len());
    let ranges = gop_indices
        .iter()
        .map(|index| {
            let gop = &probe.gops[*index];
            PacketRange {
                start_packet: gop.start_frame,
                end_packet: gop.end_frame,
            }
        })
        .collect::<Vec<_>>();
    let result = ffmpeg::export_video_ranges(
        &input_path,
        &PathBuf::from(output),
        &ranges,
        parse_export_kind(kind),
    );
    match result {
        Ok(summary) => {
            println!(
                "exported {} GOPs, {} packets to {} ({} bytes reported) in {:.0}ms",
                gop_indices.len(),
                summary.packets_written,
                summary.output_path,
                summary.bytes_written,
                started_at.elapsed().as_secs_f64() * 1000.0
            );
            Ok(())
        }
        Err(error) => {
            eprintln!("Export failed: {error}");
            std::process::exit(1);
        }
    }
}

fn handle_export_gops_ordered(args: &[String]) -> eframe::Result<()> {
    let (Some(input), Some(gops), Some(kind), Some(output)) =
        (args.get(1), args.get(2), args.get(3), args.get(4))
    else {
        eprintln!(
            "Usage: gop-composer --export-gops-ordered <input> <gop-list> <mp4|raw> <output> [--generate-pts] [--pts-step=N]"
        );
        eprintln!("Example gop-list: 2,1 or 5,3,4");
        std::process::exit(2);
    };
    let generate_pts = args.iter().any(|arg| arg == "--generate-pts");
    let pts_step = args
        .iter()
        .find_map(|arg| arg.strip_prefix("--pts-step="))
        .map(|value| parse_i64(value, "PTS step"))
        .unwrap_or(0);
    let input_path = PathBuf::from(input);
    let started_at = Instant::now();
    let probe = match ffmpeg::probe_gops(&input_path) {
        Ok(probe) => probe,
        Err(error) => {
            eprintln!("Probe failed: {error}");
            std::process::exit(1);
        }
    };
    let segments = parse_gop_sequence(gops, probe.gops.len())
        .iter()
        .map(|index| {
            let gop = &probe.gops[*index];
            ExportSegment {
                input_path: input_path.clone(),
                range: PacketRange {
                    start_packet: gop.start_frame,
                    end_packet: gop.end_frame,
                },
            }
        })
        .collect::<Vec<_>>();
    let result = ffmpeg::export_video_segments(
        &segments,
        &PathBuf::from(output),
        parse_export_kind(kind),
        ExportOptions {
            include_audio: true,
            time_base: valid_time_base(&probe),
            generate_pts,
            pts_step,
        },
    );
    match result {
        Ok(summary) => {
            println!(
                "exported {} ordered GOPs, {} packets to {} ({} bytes reported) in {:.0}ms",
                segments.len(),
                summary.packets_written,
                summary.output_path,
                summary.bytes_written,
                started_at.elapsed().as_secs_f64() * 1000.0
            );
            Ok(())
        }
        Err(error) => {
            eprintln!("Export failed: {error}");
            std::process::exit(1);
        }
    }
}

fn handle_timecode_map(args: &[String]) -> eframe::Result<()> {
    let (Some(main), Some(replacement)) = (args.get(1), args.get(2)) else {
        eprintln!("Usage: gop-composer --timecode-map <main-video> <replacement-video>");
        std::process::exit(2);
    };
    let main_probe = match ffmpeg::probe_gops(&PathBuf::from(main)) {
        Ok(probe) => probe,
        Err(error) => {
            eprintln!("Probe main failed: {error}");
            std::process::exit(1);
        }
    };
    let replacement_probe = match ffmpeg::probe_gops(&PathBuf::from(replacement)) {
        Ok(probe) => probe,
        Err(error) => {
            eprintln!("Probe replacement failed: {error}");
            std::process::exit(1);
        }
    };
    match print_timecode_map(&main_probe, &replacement_probe) {
        Ok(()) => Ok(()),
        Err(error) => {
            eprintln!("Timecode map failed: {error}");
            std::process::exit(1);
        }
    }
}

fn handle_preview_bench(args: &[String]) -> eframe::Result<()> {
    let (Some(input), Some(gop), Some(count), Some(repeats)) =
        (args.get(1), args.get(2), args.get(3), args.get(4))
    else {
        eprintln!(
            "Usage: gop-composer --bench-preview-session <input> <gop-number> <window-count> <repeats>"
        );
        std::process::exit(2);
    };
    let input_path = PathBuf::from(input);
    match preview_session_bench(
        &input_path,
        parse_i64(gop, "GOP number"),
        parse_i32(count, "window count"),
        parse_i32(repeats, "repeats"),
    ) {
        Ok(()) => Ok(()),
        Err(error) => {
            eprintln!("Preview session bench failed: {error}");
            std::process::exit(1);
        }
    }
}

fn print_export_result(
    result: Result<ffmpeg::ExportSummary, String>,
    started_at: Instant,
) -> eframe::Result<()> {
    match result {
        Ok(summary) => {
            println!(
                "exported {} packets to {} ({} bytes reported) in {:.0}ms",
                summary.packets_written,
                summary.output_path,
                summary.bytes_written,
                started_at.elapsed().as_secs_f64() * 1000.0
            );
            Ok(())
        }
        Err(error) => {
            eprintln!("Export failed: {error}");
            std::process::exit(1);
        }
    }
}
fn print_probe_summary(probe: &ProbeSummary) {
    println!(
        "{} | codec={} | {}x{} | duration={:.3}s | packets={} | gops={}",
        probe.path,
        probe.codec_name,
        probe.width,
        probe.height,
        probe.duration_seconds(),
        probe.frame_count,
        probe.gops.len()
    );
    if let Some(timecode) = &probe.timecode {
        println!(
            "Timecode {} @ {}/{} fps starts at absolute frame {}",
            timecode.value, timecode.rate_num, timecode.rate_den, timecode.start_frame
        );
    }
    for stream in &probe.streams {
        println!(
            "Stream #{} {} {} ({}) {} duration={:.3}s bitrate={}",
            stream.index,
            stream.media_type.label(),
            stream.codec_name,
            stream.codec_id,
            stream_details(stream),
            stream.duration_us as f64 / 1_000_000.0,
            if stream.bit_rate > 0 {
                format!("{} kb/s", stream.bit_rate / 1000)
            } else {
                "unknown".to_owned()
            }
        );
    }
    for gop in probe.gops.iter().take(8) {
        println!(
            "GOP {:>4}: frames {}-{} ({}) time {:.3}+{:.3}s bytes {}-{}",
            gop.index + 1,
            gop.start_frame,
            gop.end_frame,
            gop.frames(),
            gop.start_time,
            gop.duration,
            gop.byte_start,
            gop.byte_end
        );
    }
}

fn print_timecode_map(main: &ProbeSummary, replacement: &ProbeSummary) -> Result<(), String> {
    let main_timecode = main
        .timecode
        .as_ref()
        .ok_or_else(|| "main video has no readable timecode".to_owned())?;
    let replacement_timecode = replacement
        .timecode
        .as_ref()
        .ok_or_else(|| "replacement video has no readable timecode".to_owned())?;
    if (main_timecode.rate_num, main_timecode.rate_den)
        != (replacement_timecode.rate_num, replacement_timecode.rate_den)
    {
        return Err(format!(
            "timecode rates differ: main {}/{} replacement {}/{}",
            main_timecode.rate_num,
            main_timecode.rate_den,
            replacement_timecode.rate_num,
            replacement_timecode.rate_den
        ));
    }

    let main_by_timecode = main
        .gops
        .iter()
        .map(|gop| (main_timecode.start_frame + gop.start_frame, gop.index))
        .collect::<HashMap<_, _>>();

    println!(
        "main timecode {} | replacement timecode {} | rate {}/{}",
        main_timecode.value,
        replacement_timecode.value,
        main_timecode.rate_num,
        main_timecode.rate_den
    );
    for replacement_gop in &replacement.gops {
        let absolute_frame = replacement_timecode.start_frame + replacement_gop.start_frame;
        if let Some(main_index) = main_by_timecode.get(&absolute_frame) {
            println!(
                "replacement GOP {} -> main GOP {} at absolute frame {}",
                replacement_gop.index + 1,
                main_index + 1,
                absolute_frame
            );
        } else {
            println!(
                "replacement GOP {} -> no main GOP at absolute frame {}",
                replacement_gop.index + 1,
                absolute_frame
            );
        }
    }
    Ok(())
}

fn decode_window_smoke(path: &Path, start_frame: i64, frame_count: i32) -> Result<(), String> {
    let probe = ffmpeg::probe_gops(path)?;
    let anchor = anchor_gop_for_frame(&probe, start_frame)
        .ok_or_else(|| "no GOP anchor found".to_owned())?;
    let started_at = Instant::now();
    let frames = ffmpeg::decode_rgba_window(
        path,
        anchor.start_frame,
        anchor.pts,
        start_frame,
        frame_count,
    )?;
    let elapsed = started_at.elapsed();
    let first = frames.first().map(|frame| frame.frame_index).unwrap_or(-1);
    let last = frames.last().map(|frame| frame.frame_index).unwrap_or(-1);
    let dims = frames
        .first()
        .map(|frame| format!("{}x{}", frame.frame.width, frame.frame.height))
        .unwrap_or_else(|| "empty".to_owned());
    let first_checksum = frames
        .first()
        .map(|frame| rgba_checksum(&frame.frame.pixels))
        .unwrap_or(0);
    let last_checksum = frames
        .last()
        .map(|frame| rgba_checksum(&frame.frame.pixels))
        .unwrap_or(0);
    println!(
        "decoded window start={} count={} got={} first={} last={} dims={} first_checksum={:016x} last_checksum={:016x} elapsed={:.0}ms anchor_gop={} anchor_frame={} anchor_pts={}",
        start_frame,
        frame_count,
        frames.len(),
        first,
        last,
        dims,
        first_checksum,
        last_checksum,
        elapsed.as_secs_f64() * 1000.0,
        anchor.index + 1,
        anchor.start_frame,
        anchor.pts
    );
    Ok(())
}

fn preview_session_bench(
    path: &Path,
    gop_number: i64,
    frame_count: i32,
    repeats: i32,
) -> Result<(), String> {
    if gop_number <= 0 {
        return Err("GOP number must be 1-based and positive".to_owned());
    }
    if frame_count <= 0 || repeats <= 0 {
        return Err("window count and repeats must be positive".to_owned());
    }

    let probe = ffmpeg::probe_gops(path)?;
    let gop_index = (gop_number - 1) as usize;
    let gop = probe.gops.get(gop_index).ok_or_else(|| {
        format!(
            "GOP {gop_number} is out of range. Valid range is 1-{}",
            probe.gops.len()
        )
    })?;

    let stateless_started = Instant::now();
    let mut stateless_frames = 0usize;
    for _ in 0..repeats {
        let frames = ffmpeg::decode_rgba_window(
            path,
            gop.start_frame,
            gop.pts,
            gop.start_frame,
            frame_count,
        )?;
        stateless_frames += frames.len();
    }
    let stateless_elapsed = stateless_started.elapsed();

    let mut session = ffmpeg::open_preview_session(path)?;
    let accel = session.accel_name();
    let session_started = Instant::now();
    let mut session_frames = 0usize;
    for _ in 0..repeats {
        let frames =
            session.decode_window(gop.start_frame, gop.pts, gop.start_frame, frame_count)?;
        session_frames += frames.len();
    }
    let session_elapsed = session_started.elapsed();

    println!(
        "bench GOP {} window={} repeats={} stateless={} frames/{:.0}ms session={} frames/{:.0}ms accel={}",
        gop_number,
        frame_count,
        repeats,
        stateless_frames,
        stateless_elapsed.as_secs_f64() * 1000.0,
        session_frames,
        session_elapsed.as_secs_f64() * 1000.0,
        accel
    );
    Ok(())
}

fn rgba_checksum(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn parse_i64(value: &str, name: &str) -> i64 {
    match value.parse::<i64>() {
        Ok(value) => value,
        Err(error) => {
            eprintln!("Invalid {name}: {error}");
            std::process::exit(2);
        }
    }
}

fn parse_i32(value: &str, name: &str) -> i32 {
    match value.parse::<i32>() {
        Ok(value) => value,
        Err(error) => {
            eprintln!("Invalid {name}: {error}");
            std::process::exit(2);
        }
    }
}

fn parse_packet_ranges(value: &str) -> Vec<PacketRange> {
    let mut ranges = value
        .split(',')
        .filter(|part| !part.trim().is_empty())
        .map(|part| {
            let Some((start, end)) = part.trim().split_once('-') else {
                eprintln!("Invalid packet range: {part}. Expected start-end.");
                std::process::exit(2);
            };
            PacketRange {
                start_packet: parse_i64(start.trim(), "range start packet"),
                end_packet: parse_i64(end.trim(), "range end packet"),
            }
        })
        .collect::<Vec<_>>();
    ranges.sort_by_key(|range| range.start_packet);
    for pair in ranges.windows(2) {
        if pair[0].end_packet >= pair[1].start_packet {
            eprintln!("Packet ranges must be sorted and non-overlapping after normalization.");
            std::process::exit(2);
        }
    }
    if ranges.is_empty() {
        eprintln!("At least one packet range is required.");
        std::process::exit(2);
    }
    ranges
}

fn parse_gop_selection(value: &str, gop_count: usize) -> Vec<usize> {
    if gop_count == 0 {
        eprintln!("Input has no GOPs.");
        std::process::exit(2);
    }

    let mut selected = Vec::new();
    for part in value
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
    {
        if let Some((start, end)) = part.split_once('-') {
            let start = parse_gop_number(start.trim(), gop_count);
            let end = parse_gop_number(end.trim(), gop_count);
            let (first, last) = if start <= end {
                (start, end)
            } else {
                (end, start)
            };
            selected.extend(first..=last);
        } else {
            selected.push(parse_gop_number(part, gop_count));
        }
    }

    selected.sort_unstable();
    selected.dedup();
    if selected.is_empty() {
        eprintln!("At least one GOP number is required.");
        std::process::exit(2);
    }
    selected
}

fn parse_gop_sequence(value: &str, gop_count: usize) -> Vec<usize> {
    let selected = value
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| parse_gop_number(part, gop_count))
        .collect::<Vec<_>>();
    if selected.is_empty() {
        eprintln!("At least one GOP number is required.");
        std::process::exit(2);
    }
    selected
}

fn valid_time_base(probe: &ProbeSummary) -> Option<(i32, i32)> {
    if probe.time_base_num > 0 && probe.time_base_den > 0 {
        Some((probe.time_base_num, probe.time_base_den))
    } else {
        None
    }
}

fn parse_gop_number(value: &str, gop_count: usize) -> usize {
    let number = match value.parse::<usize>() {
        Ok(number) => number,
        Err(error) => {
            eprintln!("Invalid GOP number: {error}");
            std::process::exit(2);
        }
    };
    if number == 0 || number > gop_count {
        eprintln!("GOP number {number} is out of range. Valid range is 1-{gop_count}.");
        std::process::exit(2);
    }
    number - 1
}

fn parse_export_kind(value: &str) -> ExportKind {
    match value.to_ascii_lowercase().as_str() {
        "mp4" => ExportKind::Mp4,
        "raw" | "h264" | "h265" | "hevc" => ExportKind::Raw,
        other => {
            eprintln!("Invalid export kind: {other}. Expected mp4 or raw.");
            std::process::exit(2);
        }
    }
}
