#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Language {
    #[default]
    English,
    Chinese,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Text {
    language: Language,
}

pub(crate) struct ProbeProgressText<'a> {
    pub(crate) completed: usize,
    pub(crate) total: usize,
    pub(crate) pending: usize,
    pub(crate) finalizing: usize,
    pub(crate) bytes_read: &'a str,
    pub(crate) total_bytes: &'a str,
    pub(crate) percent: f32,
    pub(crate) elapsed: f32,
}

impl Language {
    pub(crate) fn toggle(self) -> Self {
        match self {
            Self::English => Self::Chinese,
            Self::Chinese => Self::English,
        }
    }
}

impl Text {
    pub(crate) fn new(language: Language) -> Self {
        Self { language }
    }

    pub(crate) fn switch_language(self) -> &'static str {
        match self.language {
            Language::English => "中文",
            Language::Chinese => "English",
        }
    }

    pub(crate) fn language_switched(self) -> &'static str {
        match self.language {
            Language::English => "Language switched to English",
            Language::Chinese => "已切换到中文",
        }
    }

    pub(crate) fn about(self) -> &'static str {
        match self.language {
            Language::English => "About",
            Language::Chinese => "关于",
        }
    }

    pub(crate) fn about_title(self) -> &'static str {
        match self.language {
            Language::English => "About GOP Composer",
            Language::Chinese => "关于 GOP Composer",
        }
    }

    pub(crate) fn author(self) -> &'static str {
        match self.language {
            Language::English => "Author",
            Language::Chinese => "作者",
        }
    }

    pub(crate) fn contact(self) -> &'static str {
        match self.language {
            Language::English => "Contact",
            Language::Chinese => "联系方式",
        }
    }

    pub(crate) fn project_intro_title(self) -> &'static str {
        match self.language {
            Language::English => "Project",
            Language::Chinese => "项目简介",
        }
    }

    pub(crate) fn project_intro(self) -> &'static str {
        match self.language {
            Language::English => {
                "GOP Composer is a local desktop tool for GOP-level replacement and remuxing of H.264/HEVC distribution streams. It is built for quick, lossless replacement of small visual ranges such as burned-in subtitle fixes, while preserving unchanged GOPs as compressed packets."
            }
            Language::Chinese => {
                "GOP Composer 是一个本地桌面工具，用于对 H.264/HEVC 分发码流进行 GOP 级替换和重新封装。它面向烧录字幕修正等小范围画面替换场景，尽量保留未修改 GOP 的原始压缩包，实现快速、无损的局部换源。"
            }
        }
    }

    pub(crate) fn local_processing_note(self) -> &'static str {
        match self.language {
            Language::English => "All media processing is local. Files are not uploaded.",
            Language::Chinese => "所有媒体处理都在本地完成，文件不会上传。",
        }
    }

    pub(crate) fn ready(self) -> &'static str {
        match self.language {
            Language::English => "Ready",
            Language::Chinese => "就绪",
        }
    }

    pub(crate) fn wait_indexing_import(self) -> &'static str {
        match self.language {
            Language::English => {
                "Wait for the current indexing job to finish before importing more videos"
            }
            Language::Chinese => "请等待当前索引任务完成后再导入更多视频",
        }
    }

    pub(crate) fn probe_failed(self, error: &str) -> String {
        match self.language {
            Language::English => format!("Probe failed: {error}"),
            Language::Chinese => format!("探测失败：{error}"),
        }
    }

    pub(crate) fn added_existing_tracks(self, count: usize) -> String {
        match self.language {
            Language::English => format!(
                "Added {count} track{} from existing source{}",
                if count == 1 { "" } else { "s" },
                if count == 1 { "" } else { "s" }
            ),
            Language::Chinese => format!("已从现有来源添加 {count} 条轨道"),
        }
    }

    pub(crate) fn added_existing_tracks_rejected(self, tracks: usize, rejected: usize) -> String {
        match self.language {
            Language::English => format!(
                "Added {tracks} existing track{}, rejected {rejected} file{}",
                if tracks == 1 { "" } else { "s" },
                if rejected == 1 { "" } else { "s" }
            ),
            Language::Chinese => format!("已添加 {tracks} 条现有轨道，拒绝 {rejected} 个文件"),
        }
    }

    pub(crate) fn indexing_files(
        self,
        pending: usize,
        existing_tracks: usize,
        rejected: usize,
    ) -> String {
        match self.language {
            Language::English => {
                let existing_text = if existing_tracks > 0 {
                    format!(
                        ", added {existing_tracks} existing track{}",
                        if existing_tracks == 1 { "" } else { "s" }
                    )
                } else {
                    String::new()
                };
                let rejected_text = if rejected > 0 {
                    format!(
                        ", rejected {rejected} file{}",
                        if rejected == 1 { "" } else { "s" }
                    )
                } else {
                    String::new()
                };
                format!(
                    "Indexing {pending} file{}{}{}",
                    if pending == 1 { "" } else { "s" },
                    existing_text,
                    rejected_text
                )
            }
            Language::Chinese => {
                let existing_text = if existing_tracks > 0 {
                    format!("，已添加 {existing_tracks} 条现有轨道")
                } else {
                    String::new()
                };
                let rejected_text = if rejected > 0 {
                    format!("，拒绝 {rejected} 个文件")
                } else {
                    String::new()
                };
                format!("正在索引 {pending} 个文件{existing_text}{rejected_text}")
            }
        }
    }

    pub(crate) fn probe_worker_disconnected(self) -> &'static str {
        match self.language {
            Language::English => "Probe worker disconnected",
            Language::Chinese => "索引工作线程已断开",
        }
    }

    pub(crate) fn probe_progress(self, progress: ProbeProgressText<'_>) -> String {
        match self.language {
            Language::English => {
                let stage = if progress.finalizing > 0 {
                    format!(" · {} finalizing index", progress.finalizing)
                } else {
                    String::new()
                };
                format!(
                    "Indexing {}/{} files · {} pending{} · {} / {} · {:.2}% · {:.1}s",
                    progress.completed,
                    progress.total,
                    progress.pending,
                    stage,
                    progress.bytes_read,
                    progress.total_bytes,
                    progress.percent,
                    progress.elapsed
                )
            }
            Language::Chinese => {
                let stage = if progress.finalizing > 0 {
                    format!(" · {} 个正在完成索引", progress.finalizing)
                } else {
                    String::new()
                };
                format!(
                    "索引 {}/{} 个文件 · {} 个待处理{} · {} / {} · {:.2}% · {:.1}s",
                    progress.completed,
                    progress.total,
                    progress.pending,
                    stage,
                    progress.bytes_read,
                    progress.total_bytes,
                    progress.percent,
                    progress.elapsed
                )
            }
        }
    }

    pub(crate) fn probe_progress_unknown(
        self,
        completed: usize,
        total: usize,
        pending: usize,
        elapsed: f32,
    ) -> String {
        match self.language {
            Language::English => format!(
                "Indexing {completed}/{total} files · {pending} pending · size unknown · {elapsed:.1}s"
            ),
            Language::Chinese => {
                format!(
                    "索引 {completed}/{total} 个文件 · {pending} 个待处理 · 大小未知 · {elapsed:.1}s"
                )
            }
        }
    }

    pub(crate) fn probe_file_failed(self, error: &str) -> String {
        match self.language {
            Language::English => format!("failed: {error}"),
            Language::Chinese => format!("失败：{error}"),
        }
    }

    pub(crate) fn done(self) -> &'static str {
        match self.language {
            Language::English => "done",
            Language::Chinese => "完成",
        }
    }

    pub(crate) fn finalizing_index(self, bytes_read: &str, total_bytes: &str) -> String {
        match self.language {
            Language::English => format!("{bytes_read} / {total_bytes} · finalizing index"),
            Language::Chinese => format!("{bytes_read} / {total_bytes} · 正在完成索引"),
        }
    }

    pub(crate) fn read_size_unknown(self, bytes_read: &str) -> String {
        match self.language {
            Language::English => format!("{bytes_read} read · size unknown"),
            Language::Chinese => format!("已读取 {bytes_read} · 大小未知"),
        }
    }

    pub(crate) fn indexed_sources_tracks_pending(
        self,
        source_added: bool,
        sources: usize,
        tracks: usize,
        pending: usize,
        elapsed: f32,
    ) -> String {
        match self.language {
            Language::English => format!(
                "{} {sources} source{}, {tracks} track{}, {pending} pending, {elapsed:.2}s",
                if source_added { "Indexed" } else { "Reused" },
                if sources == 1 { "" } else { "s" },
                if tracks == 1 { "" } else { "s" }
            ),
            Language::Chinese => format!(
                "{} {sources} 个来源，{tracks} 条轨道，{pending} 个待处理，{elapsed:.2}s",
                if source_added {
                    "已索引"
                } else {
                    "已复用"
                }
            ),
        }
    }

    pub(crate) fn closing(self) -> &'static str {
        match self.language {
            Language::English => "Closing",
            Language::Chinese => "正在关闭",
        }
    }

    pub(crate) fn no_playhead_position(self) -> &'static str {
        match self.language {
            Language::English => "No playhead position to play",
            Language::Chinese => "没有可播放的播放头位置",
        }
    }

    pub(crate) fn playback_started(self) -> &'static str {
        match self.language {
            Language::English => "Playback started",
            Language::Chinese => "已开始播放",
        }
    }

    pub(crate) fn playback_stopped_at_end(self) -> &'static str {
        match self.language {
            Language::English => "Playback stopped at timeline end",
            Language::Chinese => "已播放到时间线末尾",
        }
    }

    pub(crate) fn opened_source_location(self, name: &str) -> String {
        match self.language {
            Language::English => format!("Opened source location for {name}"),
            Language::Chinese => format!("已打开 {name} 的所在位置"),
        }
    }

    pub(crate) fn failed_open_source_location(self, error: &str) -> String {
        match self.language {
            Language::English => format!("Failed to open source location: {error}"),
            Language::Chinese => format!("打开来源位置失败：{error}"),
        }
    }

    pub(crate) fn wait_indexing_delete(self) -> &'static str {
        match self.language {
            Language::English => "Wait for indexing to finish before deleting a source",
            Language::Chinese => "请等待索引完成后再删除来源",
        }
    }

    pub(crate) fn wait_export_delete(self) -> &'static str {
        match self.language {
            Language::English => "Wait for export to finish before deleting a source",
            Language::Chinese => "请等待导出完成后再删除来源",
        }
    }

    pub(crate) fn deleted_source(self, name: &str) -> String {
        match self.language {
            Language::English => format!("Deleted source {name}"),
            Language::Chinese => format!("已删除来源 {name}"),
        }
    }

    pub(crate) fn deleted_source_tracks(self, name: &str) -> String {
        match self.language {
            Language::English => format!("Deleted source {name} and its timeline tracks"),
            Language::Chinese => format!("已删除来源 {name} 及其时间线轨道"),
        }
    }

    pub(crate) fn open_main_video(self) -> &'static str {
        match self.language {
            Language::English => "Open Main Video",
            Language::Chinese => "打开主视频",
        }
    }

    pub(crate) fn add_videos(self) -> &'static str {
        match self.language {
            Language::English => "Add videos",
            Language::Chinese => "添加视频",
        }
    }

    pub(crate) fn clear(self) -> &'static str {
        match self.language {
            Language::English => "Clear",
            Language::Chinese => "清空",
        }
    }

    pub(crate) fn play(self) -> &'static str {
        match self.language {
            Language::English => "Play",
            Language::Chinese => "播放",
        }
    }

    pub(crate) fn pause(self) -> &'static str {
        match self.language {
            Language::English => "Pause",
            Language::Chinese => "暂停",
        }
    }

    pub(crate) fn gop_list(self) -> &'static str {
        match self.language {
            Language::English => "GOP List",
            Language::Chinese => "GOP 列表",
        }
    }

    pub(crate) fn export_mp4(self) -> &'static str {
        match self.language {
            Language::English => "Export MP4",
            Language::Chinese => "导出 MP4",
        }
    }

    pub(crate) fn export_raw(self) -> &'static str {
        match self.language {
            Language::English => "Export Raw",
            Language::Chinese => "导出裸流",
        }
    }

    pub(crate) fn export_match(self) -> &'static str {
        match self.language {
            Language::English => "Export match",
            Language::Chinese => "导出匹配",
        }
    }

    pub(crate) fn export_match_ok(self) -> &'static str {
        match self.language {
            Language::English => "Video codec, size, timebase and frame rate match",
            Language::Chinese => "视频编码、尺寸、时间基和帧率一致",
        }
    }

    pub(crate) fn export_match_mismatch(self, count: usize) -> String {
        match self.language {
            Language::English => format!(
                "{count} file{} differ in video codec, size, timebase or frame rate",
                if count == 1 { "" } else { "s" }
            ),
            Language::Chinese => format!("{count} 个文件的视频编码、尺寸、时间基或帧率不一致"),
        }
    }

    pub(crate) fn inspector(self) -> &'static str {
        match self.language {
            Language::English => "Inspector",
            Language::Chinese => "检查器",
        }
    }

    pub(crate) fn current_frame(self) -> &'static str {
        match self.language {
            Language::English => "Current frame",
            Language::Chinese => "当前帧",
        }
    }

    pub(crate) fn select_gop(self) -> &'static str {
        match self.language {
            Language::English => "Select a GOP.",
            Language::Chinese => "请选择一个 GOP。",
        }
    }

    pub(crate) fn open_video_hint(self) -> &'static str {
        match self.language {
            Language::English => "Open a video file to build a GOP index.",
            Language::Chinese => "打开视频文件以建立 GOP 索引。",
        }
    }

    pub(crate) fn status(self) -> &'static str {
        match self.language {
            Language::English => "Status",
            Language::Chinese => "状态",
        }
    }

    pub(crate) fn frame(self) -> &'static str {
        match self.language {
            Language::English => "Frame",
            Language::Chinese => "帧",
        }
    }

    pub(crate) fn preview(self) -> &'static str {
        match self.language {
            Language::English => "Preview",
            Language::Chinese => "预览",
        }
    }

    pub(crate) fn mux_audio(self) -> &'static str {
        match self.language {
            Language::English => "Mux audio",
            Language::Chinese => "混合音频",
        }
    }

    pub(crate) fn source_timebase(self) -> &'static str {
        match self.language {
            Language::English => "Source TB",
            Language::Chinese => "源时间基",
        }
    }

    pub(crate) fn source_timebase_tip(self) -> &'static str {
        match self.language {
            Language::English => "Main track sources have different timebases",
            Language::Chinese => "主轨道来源的时间基不一致",
        }
    }

    pub(crate) fn generate_pts(self) -> &'static str {
        match self.language {
            Language::English => "Generate PTS",
            Language::Chinese => "生成 PTS",
        }
    }

    pub(crate) fn pts_step_tip(self) -> &'static str {
        match self.language {
            Language::English => "0 uses the video frame rate and selected timebase",
            Language::Chinese => "0 表示按视频帧率和所选时间基自动计算",
        }
    }

    pub(crate) fn sources(self) -> &'static str {
        match self.language {
            Language::English => "Sources",
            Language::Chinese => "来源",
        }
    }

    pub(crate) fn delete(self) -> &'static str {
        match self.language {
            Language::English => "Delete",
            Language::Chinese => "删除",
        }
    }

    pub(crate) fn show_in_explorer(self) -> &'static str {
        match self.language {
            Language::English => "Show in Explorer",
            Language::Chinese => "资源管理器",
        }
    }

    pub(crate) fn open_video_empty_title(self) -> &'static str {
        match self.language {
            Language::English => "Open a video to inspect GOP boundaries",
            Language::Chinese => "打开视频以检查 GOP 边界",
        }
    }

    pub(crate) fn open_video_empty_body(self) -> &'static str {
        match self.language {
            Language::English => "GOP packets are indexed locally through FFmpeg.",
            Language::Chinese => "通过 FFmpeg 在本地索引 GOP 数据包。",
        }
    }

    pub(crate) fn codec(self) -> &'static str {
        match self.language {
            Language::English => "Codec",
            Language::Chinese => "编码",
        }
    }

    pub(crate) fn size(self) -> &'static str {
        match self.language {
            Language::English => "Size",
            Language::Chinese => "尺寸",
        }
    }

    pub(crate) fn timebase(self) -> &'static str {
        match self.language {
            Language::English => "Timebase",
            Language::Chinese => "时间基",
        }
    }

    pub(crate) fn frame_rate(self) -> &'static str {
        match self.language {
            Language::English => "Frame rate",
            Language::Chinese => "帧率",
        }
    }

    pub(crate) fn avg_rate(self) -> &'static str {
        match self.language {
            Language::English => "Avg rate",
            Language::Chinese => "平均帧率",
        }
    }

    pub(crate) fn duration(self) -> &'static str {
        match self.language {
            Language::English => "Duration",
            Language::Chinese => "时长",
        }
    }

    pub(crate) fn packets(self) -> &'static str {
        match self.language {
            Language::English => "Packets",
            Language::Chinese => "包数",
        }
    }

    pub(crate) fn gops(self) -> &'static str {
        match self.language {
            Language::English => "GOPs",
            Language::Chinese => "GOP 数",
        }
    }

    pub(crate) fn timecode(self) -> &'static str {
        match self.language {
            Language::English => "Timecode",
            Language::Chinese => "时间码",
        }
    }

    pub(crate) fn streams(self) -> &'static str {
        match self.language {
            Language::English => "Streams",
            Language::Chinese => "流",
        }
    }

    pub(crate) fn stream_type(self) -> &'static str {
        match self.language {
            Language::English => "Type",
            Language::Chinese => "类型",
        }
    }

    pub(crate) fn details(self) -> &'static str {
        match self.language {
            Language::English => "Details",
            Language::Chinese => "详情",
        }
    }

    pub(crate) fn frames(self) -> &'static str {
        match self.language {
            Language::English => "Frames",
            Language::Chinese => "帧",
        }
    }

    pub(crate) fn time(self) -> &'static str {
        match self.language {
            Language::English => "Time",
            Language::Chinese => "时间",
        }
    }

    pub(crate) fn bytes(self) -> &'static str {
        match self.language {
            Language::English => "Bytes",
            Language::Chinese => "字节",
        }
    }

    pub(crate) fn start(self) -> &'static str {
        match self.language {
            Language::English => "Start",
            Language::Chinese => "起始",
        }
    }

    pub(crate) fn end(self) -> &'static str {
        match self.language {
            Language::English => "End",
            Language::Chinese => "结束",
        }
    }

    pub(crate) fn gop_timeline(self) -> &'static str {
        match self.language {
            Language::English => "GOP Timeline",
            Language::Chinese => "GOP 时间线",
        }
    }

    pub(crate) fn timeline_stats(
        self,
        tracks: usize,
        gops: usize,
        frames: i64,
        zoom: f32,
    ) -> String {
        match self.language {
            Language::English => {
                format!("{tracks} tracks / {gops} GOPs / {frames} frames / {zoom:.2} px-frame")
            }
            Language::Chinese => {
                format!("{tracks} 条轨道 / {gops} 个 GOP / {frames} 帧 / {zoom:.2} 像素-帧")
            }
        }
    }

    pub(crate) fn timeline_empty(self) -> &'static str {
        match self.language {
            Language::English => "Drop or open media to build GOP tracks",
            Language::Chinese => "拖入或打开媒体以建立 GOP 轨道",
        }
    }

    pub(crate) fn no_cached_preview(self) -> &'static str {
        match self.language {
            Language::English => "No cached preview frame",
            Language::Chinese => "没有缓存的预览帧",
        }
    }

    pub(crate) fn apply_timecode_title(self) -> &'static str {
        match self.language {
            Language::English => "Apply timecode replacement?",
            Language::Chinese => "应用时间码替换？",
        }
    }

    pub(crate) fn apply(self) -> &'static str {
        match self.language {
            Language::English => "Apply",
            Language::Chinese => "应用",
        }
    }

    pub(crate) fn cancel(self) -> &'static str {
        match self.language {
            Language::English => "Cancel",
            Language::Chinese => "取消",
        }
    }

    pub(crate) fn timecode_no_compatible_main(self, name: &str) -> String {
        match self.language {
            Language::English => format!(
                "Timecode found for {name}, but the main track has no compatible timecode GOPs"
            ),
            Language::Chinese => {
                format!("已检测到 {name} 的时间码，但主轨道没有兼容的时间码 GOP")
            }
        }
    }

    pub(crate) fn timecode_no_match(self, name: &str) -> String {
        match self.language {
            Language::English => {
                format!("Timecode found for {name}, but no GOP start matches the main track")
            }
            Language::Chinese => format!("已检测到 {name} 的时间码，但没有 GOP 起点匹配主轨道"),
        }
    }

    pub(crate) fn timecode_match_detected(self, name: &str, count: usize) -> String {
        match self.language {
            Language::English => format!(
                "Timecode match detected for {name}: {count} GOP{} can be applied",
                if count == 1 { "" } else { "s" }
            ),
            Language::Chinese => format!("检测到 {name} 的时间码匹配：可应用 {count} 个 GOP"),
        }
    }

    pub(crate) fn timecode_contains(self, name: &str, tc: &str, num: i32, den: i32) -> String {
        match self.language {
            Language::English => format!("{name} contains timecode {tc} at {num}/{den} fps."),
            Language::Chinese => format!("{name} 包含时间码 {tc}，时间码帧率 {num}/{den}。"),
        }
    }

    pub(crate) fn timecode_gops_match(self, count: usize) -> String {
        match self.language {
            Language::English => format!(
                "{count} GOP{} match the main track by timecode.",
                if count == 1 { "" } else { "s" }
            ),
            Language::Chinese => format!("{count} 个 GOP 按时间码匹配主轨道。"),
        }
    }

    pub(crate) fn timecode_gops_unmatched(self, count: usize) -> String {
        match self.language {
            Language::English => format!(
                "{count} replacement GOP{} did not match.",
                if count == 1 { "" } else { "s" }
            ),
            Language::Chinese => format!("{count} 个替换 GOP 未匹配。"),
        }
    }

    pub(crate) fn absolute_frame_range(self, first: i64, last: i64) -> String {
        match self.language {
            Language::English => format!("Absolute frame range: {first} - {last}"),
            Language::Chinese => format!("绝对帧范围：{first} - {last}"),
        }
    }

    pub(crate) fn skipped_timecode_apply(self, name: &str) -> String {
        match self.language {
            Language::English => format!("Skipped timecode apply for {name}"),
            Language::Chinese => format!("已跳过 {name} 的时间码应用"),
        }
    }

    pub(crate) fn replacement_source_missing(self) -> &'static str {
        match self.language {
            Language::English => "Replacement source no longer exists",
            Language::Chinese => "替换来源已不存在",
        }
    }

    pub(crate) fn main_track_empty(self) -> &'static str {
        match self.language {
            Language::English => "Main track is empty",
            Language::Chinese => "主轨道为空",
        }
    }

    pub(crate) fn no_timecode_mapped_applied(self, name: &str) -> String {
        match self.language {
            Language::English => {
                format!("No timecode-mapped GOPs could be applied from {name}")
            }
            Language::Chinese => format!("没有可从 {name} 应用的时间码映射 GOP"),
        }
    }

    pub(crate) fn applied_timecode_mapped(self, name: &str, count: usize) -> String {
        match self.language {
            Language::English => format!(
                "Applied {count} timecode-mapped GOP{} from {name}",
                if count == 1 { "" } else { "s" }
            ),
            Language::Chinese => format!("已从 {name} 应用 {count} 个时间码映射 GOP"),
        }
    }
}
