use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct GopInfo {
    pub index: usize,
    pub start_frame: i64,
    pub end_frame: i64,
    pub pts: i64,
    pub dts: i64,
    pub byte_start: i64,
    pub byte_end: i64,
    pub start_time: f64,
    pub duration: f64,
}

impl GopInfo {
    pub fn frames(&self) -> i64 {
        (self.end_frame - self.start_frame + 1).max(0)
    }

    pub fn bytes(&self) -> i64 {
        (self.byte_end - self.byte_start).max(0)
    }
}

#[derive(Clone, Debug)]
pub struct ProbeSummary {
    pub path: String,
    pub codec_id: i32,
    pub codec_name: String,
    pub width: i32,
    pub height: i32,
    pub time_base_num: i32,
    pub time_base_den: i32,
    pub avg_frame_rate_num: i32,
    pub avg_frame_rate_den: i32,
    pub real_frame_rate_num: i32,
    pub real_frame_rate_den: i32,
    pub duration_us: i64,
    pub frame_count: i64,
    pub timecode: Option<TimecodeInfo>,
    pub gops: Vec<GopInfo>,
    pub streams: Vec<StreamInfo>,
}

impl ProbeSummary {
    pub fn duration_seconds(&self) -> f64 {
        self.duration_us as f64 / 1_000_000.0
    }
}

#[derive(Clone, Debug)]
pub struct TimecodeInfo {
    pub value: String,
    pub start_frame: i64,
    pub rate_num: i32,
    pub rate_den: i32,
}

#[derive(Clone, Debug)]
pub struct StreamInfo {
    pub index: i32,
    pub media_type: MediaType,
    pub codec_id: i32,
    pub codec_name: String,
    pub width: i32,
    pub height: i32,
    pub sample_rate: i32,
    pub channels: i32,
    pub duration_us: i64,
    pub bit_rate: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaType {
    Unknown,
    Video,
    Audio,
    Data,
    Subtitle,
    Attachment,
    Other(i32),
}

impl MediaType {
    pub fn label(self) -> &'static str {
        match self {
            MediaType::Unknown => "unknown",
            MediaType::Video => "video",
            MediaType::Audio => "audio",
            MediaType::Data => "data",
            MediaType::Subtitle => "subtitle",
            MediaType::Attachment => "attachment",
            MediaType::Other(_) => "other",
        }
    }
}

#[derive(Clone, Debug)]
pub struct RgbaFrame {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct DecodedFrame {
    pub frame_index: i64,
    pub frame: RgbaFrame,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportKind {
    Mp4,
    Raw,
}

impl ExportKind {
    pub(super) fn as_ffi(self) -> i32 {
        match self {
            ExportKind::Mp4 => 0,
            ExportKind::Raw => 1,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ExportSummary {
    pub output_path: String,
    pub packets_written: i64,
    pub bytes_written: i64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExportOptions {
    pub include_audio: bool,
    pub time_base: Option<(i32, i32)>,
    pub generate_pts: bool,
    pub pts_step: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PacketRange {
    pub start_packet: i64,
    pub end_packet: i64,
}

#[derive(Clone, Debug)]
pub struct ExportSegment {
    pub input_path: PathBuf,
    pub range: PacketRange,
}
