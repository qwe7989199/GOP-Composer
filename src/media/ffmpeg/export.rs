use std::path::Path;

use super::{
    ffi,
    output::ExportOutput,
    types::{ExportKind, ExportOptions, ExportSegment, ExportSummary, PacketRange},
};

pub fn export_video_range(
    input_path: &Path,
    output_path: &Path,
    start_packet: i64,
    end_packet: i64,
    kind: ExportKind,
) -> Result<ExportSummary, String> {
    export_video_ranges(
        input_path,
        output_path,
        &[PacketRange {
            start_packet,
            end_packet,
        }],
        kind,
    )
}

pub fn export_video_ranges(
    input_path: &Path,
    output_path: &Path,
    ranges: &[PacketRange],
    kind: ExportKind,
) -> Result<ExportSummary, String> {
    if ranges.is_empty() {
        return Err("export requires at least one packet range".to_owned());
    }

    let (_input_text, input) = ffi::path_to_cstring(input_path)?;
    let staged = ExportOutput::new(output_path, [input_path])?;
    let (output_text, _) = ffi::path_to_cstring(output_path)?;
    let (_, output) = ffi::path_to_cstring(staged.path())?;
    let raw_ranges = ranges
        .iter()
        .map(|range| ffi::GcPacketRange {
            start_packet: range.start_packet,
            end_packet: range.end_packet,
        })
        .collect::<Vec<_>>();
    let mut raw = ffi::empty_export_result();

    let code = unsafe {
        ffi::gopcomposer_export_video_ranges(
            input.as_ptr(),
            output.as_ptr(),
            raw_ranges.as_ptr(),
            raw_ranges.len() as u32,
            kind.as_ffi(),
            &mut raw,
        )
    };
    let result = export_result_from_raw(code, output_text, &raw);

    unsafe {
        ffi::gopcomposer_free_export(&mut raw);
    }
    let summary = result?;
    staged.finish()?;
    Ok(summary)
}

pub fn export_video_segments(
    segments: &[ExportSegment],
    output_path: &Path,
    kind: ExportKind,
    options: ExportOptions,
) -> Result<ExportSummary, String> {
    if segments.is_empty() {
        return Err("export requires at least one segment".to_owned());
    }

    let staged = ExportOutput::new(output_path, segments.iter().map(|s| s.input_path.as_path()))?;
    let (output_text, _) = ffi::path_to_cstring(output_path)?;
    let (_, output) = ffi::path_to_cstring(staged.path())?;
    let mut input_paths = Vec::with_capacity(segments.len());
    for segment in segments {
        input_paths.push(ffi::path_to_cstring(&segment.input_path)?.1);
    }
    let raw_segments = segments
        .iter()
        .zip(input_paths.iter())
        .map(|(segment, input)| ffi::GcExportInputSegment {
            input_path: input.as_ptr(),
            range: ffi::GcPacketRange {
                start_packet: segment.range.start_packet,
                end_packet: segment.range.end_packet,
            },
        })
        .collect::<Vec<_>>();
    let mut raw = ffi::empty_export_result();
    let raw_options = ffi::GcExportOptions {
        include_audio: i32::from(options.include_audio),
        time_base_num: options.time_base.map(|value| value.0).unwrap_or(0),
        time_base_den: options.time_base.map(|value| value.1).unwrap_or(0),
        generate_pts: i32::from(options.generate_pts),
        pts_step: options.pts_step,
    };

    let code = unsafe {
        ffi::gopcomposer_export_video_segments(
            raw_segments.as_ptr(),
            raw_segments.len() as u32,
            output.as_ptr(),
            kind.as_ffi(),
            &raw_options,
            &mut raw,
        )
    };
    let result = export_result_from_raw(code, output_text, &raw);

    unsafe {
        ffi::gopcomposer_free_export(&mut raw);
    }
    let summary = result?;
    staged.finish()?;
    Ok(summary)
}

fn export_result_from_raw(
    code: i32,
    output_text: String,
    raw: &ffi::GcExportResult,
) -> Result<ExportSummary, String> {
    if code < 0 {
        Err(ffi::c_string_lossy(raw.error)
            .unwrap_or_else(|| format!("FFmpeg export failed with code {code}")))
    } else {
        Ok(ExportSummary {
            output_path: output_text,
            packets_written: raw.packets_written,
            bytes_written: raw.bytes_written,
        })
    }
}
