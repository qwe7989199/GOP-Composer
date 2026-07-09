use std::{ffi::c_void, path::Path};

use super::{
    ffi,
    types::{GopInfo, MediaType, ProbeSummary, StreamInfo, TimecodeInfo},
};

fn probe_error(code: i32, raw: &ffi::GcProbeResult) -> String {
    ffi::c_string_lossy(raw.error)
        .unwrap_or_else(|| format!("FFmpeg probe failed with code {code}"))
}

fn probe_summary_from_raw(path_text: String, raw: &ffi::GcProbeResult) -> ProbeSummary {
    let gops = if raw.gops.is_null() || raw.gop_count == 0 {
        Vec::new()
    } else {
        unsafe { std::slice::from_raw_parts(raw.gops, raw.gop_count as usize) }
            .iter()
            .enumerate()
            .map(|(index, item)| GopInfo {
                index,
                start_frame: item.start_frame,
                end_frame: item.end_frame,
                pts: item.pts,
                dts: item.dts,
                byte_start: item.byte_start,
                byte_end: item.byte_end,
                start_time: item.start_time,
                duration: item.duration,
            })
            .collect()
    };
    let timecode = if raw.has_timecode != 0 && !raw.timecode.is_null() {
        Some(TimecodeInfo {
            value: ffi::c_string_lossy(raw.timecode).unwrap_or_default(),
            start_frame: raw.timecode_start_frame,
            rate_num: raw.timecode_rate_num,
            rate_den: raw.timecode_rate_den,
        })
    } else {
        None
    };
    let streams = if raw.streams.is_null() || raw.stream_count == 0 {
        Vec::new()
    } else {
        unsafe { std::slice::from_raw_parts(raw.streams, raw.stream_count as usize) }
            .iter()
            .map(|item| StreamInfo {
                index: item.index,
                media_type: media_type(item.media_type),
                codec_id: item.codec_id,
                codec_name: codec_name(item.codec_id),
                width: item.width,
                height: item.height,
                sample_rate: item.sample_rate,
                channels: item.channels,
                duration_us: item.duration_us,
                bit_rate: item.bit_rate,
            })
            .collect()
    };
    ProbeSummary {
        path: path_text,
        codec_id: raw.codec_id,
        codec_name: codec_name(raw.codec_id),
        width: raw.width,
        height: raw.height,
        time_base_num: raw.time_base_num,
        time_base_den: raw.time_base_den,
        avg_frame_rate_num: raw.avg_frame_rate_num,
        avg_frame_rate_den: raw.avg_frame_rate_den,
        real_frame_rate_num: raw.real_frame_rate_num,
        real_frame_rate_den: raw.real_frame_rate_den,
        duration_us: raw.duration_us,
        frame_count: raw.frame_count,
        timecode,
        gops,
        streams,
    }
}

extern "C" fn probe_progress_trampoline(user_data: *mut c_void, bytes_read: i64, total_bytes: i64) {
    if user_data.is_null() {
        return;
    }
    let callback = unsafe { &mut *(user_data as *mut &mut dyn FnMut(i64, i64)) };
    callback(bytes_read, total_bytes);
}

pub fn probe_gops(path: &Path) -> Result<ProbeSummary, String> {
    let (path_text, input) = ffi::path_to_cstring(path)?;
    let mut raw = ffi::empty_probe_result();

    let code = unsafe { ffi::gopcomposer_probe_gops(input.as_ptr(), &mut raw) };
    let result = if code < 0 {
        Err(probe_error(code, &raw))
    } else {
        Ok(probe_summary_from_raw(path_text, &raw))
    };

    unsafe {
        ffi::gopcomposer_free_probe(&mut raw);
    }
    result
}

pub fn probe_video_stream(path: &Path) -> Result<ProbeSummary, String> {
    let (path_text, input) = ffi::path_to_cstring(path)?;
    let mut raw = ffi::empty_probe_result();

    let code = unsafe { ffi::gopcomposer_probe_video_stream(input.as_ptr(), &mut raw) };
    let result = if code < 0 {
        Err(probe_error(code, &raw))
    } else {
        Ok(probe_summary_from_raw(path_text, &raw))
    };

    unsafe {
        ffi::gopcomposer_free_probe(&mut raw);
    }
    result
}

pub fn probe_gops_with_progress(
    path: &Path,
    mut on_progress: impl FnMut(i64, i64),
) -> Result<ProbeSummary, String> {
    let (path_text, input) = ffi::path_to_cstring(path)?;
    let mut raw = ffi::empty_probe_result();
    let mut callback: &mut dyn FnMut(i64, i64) = &mut on_progress;

    let code = unsafe {
        ffi::gopcomposer_probe_gops_with_progress(
            input.as_ptr(),
            &mut raw,
            Some(probe_progress_trampoline),
            &mut callback as *mut _ as *mut c_void,
        )
    };
    let result = if code < 0 {
        Err(probe_error(code, &raw))
    } else {
        Ok(probe_summary_from_raw(path_text, &raw))
    };

    unsafe {
        ffi::gopcomposer_free_probe(&mut raw);
    }
    result
}

fn codec_name(codec_id: i32) -> String {
    let ptr = unsafe { ffi::gopcomposer_codec_name(codec_id) };
    ffi::c_string_lossy(ptr).unwrap_or_else(|| "unknown".to_owned())
}

fn media_type(value: i32) -> MediaType {
    match value {
        -1 => MediaType::Unknown,
        0 => MediaType::Video,
        1 => MediaType::Audio,
        2 => MediaType::Data,
        3 => MediaType::Subtitle,
        4 => MediaType::Attachment,
        other => MediaType::Other(other),
    }
}
