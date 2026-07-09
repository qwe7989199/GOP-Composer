use std::{
    ffi::{CStr, CString, c_char, c_void},
    path::Path,
    ptr,
};

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub(super) struct GcGopInfo {
    pub(super) start_frame: i64,
    pub(super) end_frame: i64,
    pub(super) pts: i64,
    pub(super) dts: i64,
    pub(super) byte_start: i64,
    pub(super) byte_end: i64,
    pub(super) start_time: f64,
    pub(super) duration: f64,
    pub(super) key_flags: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub(super) struct GcStreamInfo {
    pub(super) index: i32,
    pub(super) media_type: i32,
    pub(super) codec_id: i32,
    pub(super) width: i32,
    pub(super) height: i32,
    pub(super) sample_rate: i32,
    pub(super) channels: i32,
    pub(super) duration_us: i64,
    pub(super) bit_rate: i64,
}

#[repr(C)]
#[derive(Debug)]
pub(super) struct GcProbeResult {
    pub(super) codec_id: i32,
    pub(super) width: i32,
    pub(super) height: i32,
    pub(super) time_base_num: i32,
    pub(super) time_base_den: i32,
    pub(super) avg_frame_rate_num: i32,
    pub(super) avg_frame_rate_den: i32,
    pub(super) real_frame_rate_num: i32,
    pub(super) real_frame_rate_den: i32,
    pub(super) duration_us: i64,
    pub(super) frame_count: i64,
    pub(super) has_timecode: i32,
    pub(super) timecode_start_frame: i64,
    pub(super) timecode_rate_num: i32,
    pub(super) timecode_rate_den: i32,
    pub(super) timecode: *mut c_char,
    pub(super) gop_count: u32,
    pub(super) gops: *mut GcGopInfo,
    pub(super) stream_count: u32,
    pub(super) streams: *mut GcStreamInfo,
    pub(super) error: *mut c_char,
}

#[repr(C)]
#[derive(Debug)]
pub(super) struct GcRgbaFrame {
    pub(super) width: i32,
    pub(super) height: i32,
    pub(super) stride: i32,
    pub(super) pixels: *mut u8,
    pub(super) error: *mut c_char,
}

#[repr(C)]
#[derive(Debug)]
pub(super) struct GcRgbaFrameItem {
    pub(super) frame_index: i64,
    pub(super) frame: GcRgbaFrame,
}

#[repr(C)]
#[derive(Debug)]
pub(super) struct GcRgbaWindow {
    pub(super) frame_count: u32,
    pub(super) frames: *mut GcRgbaFrameItem,
    pub(super) error: *mut c_char,
}

#[repr(C)]
pub(super) struct GcPreviewSession {
    _private: [u8; 0],
}

#[repr(C)]
#[derive(Debug)]
pub(super) struct GcExportResult {
    pub(super) packets_written: i64,
    pub(super) bytes_written: i64,
    pub(super) error: *mut c_char,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub(super) struct GcPacketRange {
    pub(super) start_packet: i64,
    pub(super) end_packet: i64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub(super) struct GcExportInputSegment {
    pub(super) input_path: *const c_char,
    pub(super) range: GcPacketRange,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub(super) struct GcExportOptions {
    pub(super) include_audio: i32,
    pub(super) time_base_num: i32,
    pub(super) time_base_den: i32,
    pub(super) generate_pts: i32,
    pub(super) pts_step: i64,
}

pub(super) type GcProbeProgressCallback = Option<extern "C" fn(*mut c_void, i64, i64)>;

unsafe extern "C" {
    pub(super) fn gopcomposer_probe_gops(path: *const c_char, result: *mut GcProbeResult) -> i32;
    pub(super) fn gopcomposer_probe_video_stream(
        path: *const c_char,
        result: *mut GcProbeResult,
    ) -> i32;
    pub(super) fn gopcomposer_probe_gops_with_progress(
        path: *const c_char,
        result: *mut GcProbeResult,
        callback: GcProbeProgressCallback,
        user_data: *mut c_void,
    ) -> i32;
    pub(super) fn gopcomposer_free_probe(result: *mut GcProbeResult);
    pub(super) fn gopcomposer_codec_name(codec_id: i32) -> *const c_char;
    pub(super) fn gopcomposer_decode_rgba_frame(
        path: *const c_char,
        target_frame: i64,
        result: *mut GcRgbaFrame,
    ) -> i32;
    pub(super) fn gopcomposer_free_rgba_frame(result: *mut GcRgbaFrame);
    pub(super) fn gopcomposer_decode_rgba_window(
        path: *const c_char,
        anchor_frame: i64,
        anchor_pts: i64,
        start_frame: i64,
        frame_count: i32,
        result: *mut GcRgbaWindow,
    ) -> i32;
    pub(super) fn gopcomposer_free_rgba_window(result: *mut GcRgbaWindow);
    pub(super) fn gopcomposer_preview_session_open(
        path: *const c_char,
        session_out: *mut *mut GcPreviewSession,
        error_out: *mut GcRgbaWindow,
    ) -> i32;
    pub(super) fn gopcomposer_preview_session_decode_window(
        session: *mut GcPreviewSession,
        anchor_frame: i64,
        anchor_pts: i64,
        start_frame: i64,
        frame_count: i32,
        result: *mut GcRgbaWindow,
    ) -> i32;
    pub(super) fn gopcomposer_preview_session_accel(
        session: *mut GcPreviewSession,
    ) -> *const c_char;
    pub(super) fn gopcomposer_preview_session_free(session: *mut GcPreviewSession);
    pub(super) fn gopcomposer_preview_session_prepare_dx(
        session: *mut GcPreviewSession,
        anchor_frame: i64,
        anchor_pts: i64,
        target_frame: i64,
        error_out: *mut *mut c_char,
    ) -> i32;
    pub(super) fn gopcomposer_free_string(text: *mut c_char);
    pub(super) fn gopcomposer_export_video_ranges(
        input_path: *const c_char,
        output_path: *const c_char,
        ranges: *const GcPacketRange,
        range_count: u32,
        output_kind: i32,
        result: *mut GcExportResult,
    ) -> i32;
    pub(super) fn gopcomposer_export_video_segments(
        segments: *const GcExportInputSegment,
        segment_count: u32,
        output_path: *const c_char,
        output_kind: i32,
        options: *const GcExportOptions,
        result: *mut GcExportResult,
    ) -> i32;
    pub(super) fn gopcomposer_free_export(result: *mut GcExportResult);
}

pub(super) fn path_to_cstring(path: &Path) -> Result<(String, CString), String> {
    let path_text = path
        .to_str()
        .ok_or_else(|| "input path is not valid UTF-8".to_owned())?
        .to_owned();
    let input = CString::new(path_text.as_str())
        .map_err(|_| "input path contains a NUL byte".to_owned())?;
    Ok((path_text, input))
}

pub(super) fn c_string_lossy(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() {
        None
    } else {
        Some(unsafe { CStr::from_ptr(ptr).to_string_lossy().into_owned() })
    }
}

pub(super) fn empty_probe_result() -> GcProbeResult {
    GcProbeResult {
        codec_id: 0,
        width: 0,
        height: 0,
        time_base_num: 0,
        time_base_den: 0,
        avg_frame_rate_num: 0,
        avg_frame_rate_den: 0,
        real_frame_rate_num: 0,
        real_frame_rate_den: 0,
        duration_us: 0,
        frame_count: 0,
        has_timecode: 0,
        timecode_start_frame: 0,
        timecode_rate_num: 0,
        timecode_rate_den: 0,
        timecode: ptr::null_mut(),
        gop_count: 0,
        gops: ptr::null_mut(),
        stream_count: 0,
        streams: ptr::null_mut(),
        error: ptr::null_mut(),
    }
}

pub(super) fn empty_rgba_frame() -> GcRgbaFrame {
    GcRgbaFrame {
        width: 0,
        height: 0,
        stride: 0,
        pixels: ptr::null_mut(),
        error: ptr::null_mut(),
    }
}

pub(super) fn empty_rgba_window() -> GcRgbaWindow {
    GcRgbaWindow {
        frame_count: 0,
        frames: ptr::null_mut(),
        error: ptr::null_mut(),
    }
}

pub(super) fn empty_export_result() -> GcExportResult {
    GcExportResult {
        packets_written: 0,
        bytes_written: 0,
        error: ptr::null_mut(),
    }
}
