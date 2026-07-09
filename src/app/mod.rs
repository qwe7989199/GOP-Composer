use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    path::PathBuf,
    sync::mpsc::{self, Receiver},
    thread,
    time::{Duration, Instant},
};

use eframe::egui;

use crate::{
    media::ffmpeg::{
        self, DecodedFrame, ExportKind, ExportOptions, ExportSegment, ExportSummary, PacketRange,
        ProbeSummary,
    },
    preview::{
        gl::GlPreviewState,
        worker::{PreviewRequest, PreviewWorker, spawn_preview_worker},
    },
    ui::i18n::{Language, Text},
    util::{export_kind_label, find_gop_index, track_color},
};

mod close;
mod keyboard;
mod playback;
mod preview;
mod probe;
mod sources;
mod timecode;
mod ui;

use timecode::TimecodeApplyPlan;

pub(crate) struct GopComposerApp {
    pub(crate) language: Language,
    pub(crate) sources: Vec<SourceEntry>,
    pub(crate) timeline_tracks: Vec<TimelineTrack>,
    pub(crate) active_source: Option<usize>,
    pub(crate) selected_path: Option<PathBuf>,
    pub(crate) probe: Option<ProbeSummary>,
    pub(crate) selected_gop: Option<usize>,
    pub(crate) edit_selection: BTreeSet<TimelineSelectionKey>,
    pub(crate) selection_anchor: Option<TimelineSelectionKey>,
    pub(crate) current_frame: Option<i64>,
    pub(crate) status: String,
    pub(crate) task: Option<Receiver<ProbeMessage>>,
    pub(crate) probe_replace_existing: bool,
    pub(crate) pending_timecode_apply: Option<TimecodeApplyPlan>,
    pub(crate) timecode_apply_queue: VecDeque<TimecodeApplyPlan>,
    pub(crate) preview_worker: Option<PreviewWorker>,
    pub(crate) preview_request: Option<PreviewRequest>,
    pub(crate) preview_pending_frame: Option<i64>,
    pub(crate) preview_cache: BTreeMap<i64, egui::TextureHandle>,
    pub(crate) preview_pending_decoded: VecDeque<DecodedFrame>,
    pub(crate) preview_display_frame: Option<i64>,
    pub(crate) preview_status: String,
    pub(crate) gl_preview: GlPreviewState,
    pub(crate) export_task: Option<Receiver<ExportMessage>>,
    pub(crate) export_status: String,
    pub(crate) export_include_audio: bool,
    pub(crate) export_use_source_time_base: bool,
    pub(crate) export_time_base_num: i32,
    pub(crate) export_time_base_den: i32,
    pub(crate) export_generate_pts: bool,
    pub(crate) export_pts_step: i64,
    pub(crate) playback_active: bool,
    pub(crate) playback_last_tick: Option<Instant>,
    pub(crate) task_started_at: Option<Instant>,
    pub(crate) pending_probes: usize,
    pub(crate) total_probes: usize,
    pub(crate) probe_progress: BTreeMap<PathBuf, ProbeProgress>,
    pub(crate) timeline_zoom: f32,
    pub(crate) show_gop_table: bool,
    pub(crate) timeline_clipboard: Option<TimelineItem>,
    pub(crate) undo_stack: Vec<TimelineEditSnapshot>,
    pub(crate) redo_stack: Vec<TimelineEditSnapshot>,
    pub(crate) show_about: bool,
    pub(crate) close_requested: bool,
}

impl Default for GopComposerApp {
    fn default() -> Self {
        Self {
            language: Language::default(),
            sources: Vec::new(),
            timeline_tracks: Vec::new(),
            active_source: None,
            selected_path: None,
            probe: None,
            selected_gop: None,
            edit_selection: BTreeSet::new(),
            selection_anchor: None,
            current_frame: None,
            status: Text::new(Language::default()).ready().to_owned(),
            task: None,
            probe_replace_existing: false,
            pending_timecode_apply: None,
            timecode_apply_queue: VecDeque::new(),
            preview_worker: None,
            preview_request: None,
            preview_pending_frame: None,
            preview_cache: BTreeMap::new(),
            preview_pending_decoded: VecDeque::new(),
            preview_display_frame: None,
            preview_status: String::new(),
            gl_preview: GlPreviewState::default(),
            export_task: None,
            export_status: String::new(),
            export_include_audio: false,
            export_use_source_time_base: true,
            export_time_base_num: 1,
            export_time_base_den: 90000,
            export_generate_pts: false,
            export_pts_step: 0,
            playback_active: false,
            playback_last_tick: None,
            task_started_at: None,
            pending_probes: 0,
            total_probes: 0,
            probe_progress: BTreeMap::new(),
            timeline_zoom: 1.0,
            show_gop_table: false,
            timeline_clipboard: None,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            show_about: false,
            close_requested: false,
        }
    }
}

#[derive(Clone)]
pub(crate) struct SourceEntry {
    pub(crate) path: PathBuf,
    pub(crate) name: String,
    pub(crate) probe: ProbeSummary,
    pub(crate) color: egui::Color32,
    pub(crate) selected_gop: Option<usize>,
    pub(crate) current_frame: Option<i64>,
}

#[derive(Clone)]
pub(crate) struct TimelineTrack {
    pub(crate) name: String,
    pub(crate) color: egui::Color32,
    pub(crate) items: Vec<TimelineItem>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct TimelineItem {
    pub(crate) source_index: usize,
    pub(crate) gop_index: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct TimelineSelectionKey {
    pub(crate) track_index: usize,
    pub(crate) item_index: usize,
}

#[derive(Clone)]
pub(crate) struct TimelineEditSnapshot {
    pub(crate) tracks: Vec<TimelineTrack>,
    pub(crate) edit_selection: BTreeSet<TimelineSelectionKey>,
    pub(crate) selection_anchor: Option<TimelineSelectionKey>,
    pub(crate) active_source: Option<usize>,
    pub(crate) selected_gop: Option<usize>,
    pub(crate) current_frame: Option<i64>,
}

impl SourceEntry {
    pub(crate) fn new(path: PathBuf, probe: ProbeSummary, index: usize) -> Self {
        let selected_gop = if probe.gops.is_empty() { None } else { Some(0) };
        let current_frame = probe.gops.first().map(|gop| gop.start_frame).or(Some(0));
        Self {
            name: path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("source")
                .to_owned(),
            path,
            probe,
            color: track_color(index),
            selected_gop,
            current_frame,
        }
    }
}

impl TimelineTrack {
    pub(crate) fn from_source(source: &SourceEntry, source_index: usize) -> Self {
        Self {
            name: source.name.clone(),
            color: source.color,
            items: source
                .probe
                .gops
                .iter()
                .map(|gop| TimelineItem {
                    source_index,
                    gop_index: gop.index,
                })
                .collect(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum TimelineAction {
    Seek {
        track_index: usize,
        item_index: usize,
        source_index: usize,
        frame: i64,
        gop_index: usize,
        modifiers: egui::Modifiers,
    },
    Zoom {
        zoom: f32,
    },
    MoveGop {
        from_track: usize,
        to_track: usize,
        item_index: usize,
        insert_index: usize,
    },
    MoveSelectedGops {
        to_track: usize,
        insert_index: usize,
    },
}

pub(crate) enum ProbeMessage {
    Progress {
        path: PathBuf,
        bytes_read: i64,
        total_bytes: i64,
    },
    Done {
        path: PathBuf,
        result: Result<ProbeSummary, String>,
    },
}

#[derive(Clone, Debug)]
pub(crate) struct ProbeProgress {
    pub(crate) name: String,
    pub(crate) bytes_read: i64,
    pub(crate) total_bytes: i64,
    pub(crate) done: bool,
    pub(crate) error: Option<String>,
}

pub(crate) enum ExportMessage {
    Done {
        kind: ExportKind,
        gop_index: usize,
        gop_count: usize,
        result: Result<ExportSummary, String>,
        elapsed: Duration,
    },
}

#[derive(Clone)]
struct MainTrackExportPlan {
    segments: Vec<ExportSegment>,
    first_gop: usize,
    last_gop: usize,
    codec_name: String,
    source_time_base: Option<(i32, i32)>,
    time_base_consistent: bool,
}

impl eframe::App for GopComposerApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.update_app_ui(ctx, frame);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.finish_close();
    }
}

impl GopComposerApp {
    pub(crate) fn text(&self) -> Text {
        Text::new(self.language)
    }

    pub(crate) fn toggle_language(&mut self) {
        self.language = self.language.toggle();
        self.status = self.text().language_switched().to_owned();
    }

    pub(crate) fn sync_active_source_state(&mut self) {
        let Some(index) = self.active_source else {
            return;
        };
        let Some(source) = self.sources.get_mut(index) else {
            return;
        };
        source.selected_gop = self.selected_gop;
        source.current_frame = self.current_frame;
    }

    pub(crate) fn clear_active_source_state(&mut self) {
        self.stop_playback();
        self.selected_path = None;
        self.probe = None;
        self.selected_gop = None;
        self.edit_selection.clear();
        self.selection_anchor = None;
        self.current_frame = None;
        self.preview_request = None;
        self.preview_worker = None;
        self.preview_pending_frame = None;
        self.preview_cache.clear();
        self.preview_pending_decoded.clear();
        self.preview_display_frame = None;
        self.preview_status.clear();
        self.gl_preview.clear_media();
    }

    pub(crate) fn compatibility_ui(&self, ui: &mut egui::Ui) {
        let text = self.text();
        if self.sources.len() <= 1 {
            return;
        }
        let Some(first) = self.sources.first() else {
            return;
        };
        let mismatches = self
            .sources
            .iter()
            .filter(|source| !video_export_compatible(&first.probe, &source.probe))
            .count();
        ui.separator();
        if mismatches == 0 {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(text.export_match()).strong());
                ui.label(text.export_match_ok());
            });
        } else {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(text.export_match()).strong());
                ui.label(
                    egui::RichText::new(text.export_match_mismatch(mismatches))
                        .color(egui::Color32::from_rgb(190, 73, 96))
                        .strong(),
                );
            });
        }
    }

    pub(crate) fn activate_source(&mut self, index: usize, ctx: &egui::Context) {
        if index >= self.sources.len() {
            return;
        }
        self.sync_active_source_state();

        let source = &self.sources[index];
        self.active_source = Some(index);
        self.selected_path = Some(source.path.clone());
        self.probe = Some(source.probe.clone());
        self.selected_gop = source.selected_gop;
        self.current_frame = source.current_frame;
        self.preview_request = None;
        self.preview_pending_frame = None;
        self.preview_cache.clear();
        self.preview_pending_decoded.clear();
        self.preview_display_frame = None;
        self.preview_status.clear();
        self.preview_worker = Some(spawn_preview_worker(source.path.clone()));
        self.ensure_current_frame_cached(ctx);
    }

    pub(crate) fn move_gop_between_tracks(
        &mut self,
        from_track: usize,
        to_track: usize,
        item_index: usize,
        insert_index: usize,
        ctx: &egui::Context,
    ) {
        if from_track >= self.timeline_tracks.len() || to_track >= self.timeline_tracks.len() {
            return;
        }
        self.sync_active_source_state();

        if item_index >= self.timeline_tracks[from_track].items.len() {
            return;
        }
        self.push_timeline_undo();
        let moved = self.timeline_tracks[from_track].items.remove(item_index);
        let mut insert_index = insert_index.min(self.timeline_tracks[to_track].items.len());
        if from_track == to_track && insert_index > item_index {
            insert_index = insert_index.saturating_sub(1);
        }
        self.timeline_tracks[to_track]
            .items
            .insert(insert_index, moved);
        self.edit_selection.clear();
        self.selection_anchor = Some(TimelineSelectionKey {
            track_index: to_track,
            item_index: insert_index,
        });

        let Some(start_frame) = self
            .sources
            .get(moved.source_index)
            .and_then(|source| source.probe.gops.get(moved.gop_index))
            .map(|gop| gop.start_frame)
        else {
            return;
        };

        self.status = format!(
            "Moved GOP {} from track {} to track {} at position {}",
            moved.gop_index + 1,
            from_track + 1,
            to_track + 1,
            insert_index + 1
        );
        self.activate_source(moved.source_index, ctx);
        self.set_current_frame(start_frame, ctx);
    }

    pub(crate) fn move_selected_gops(
        &mut self,
        to_track: usize,
        insert_index: usize,
        ctx: &egui::Context,
    ) {
        if self.edit_selection.is_empty() || to_track >= self.timeline_tracks.len() {
            return;
        }
        self.sync_active_source_state();
        let selected = self.edit_selection.clone();
        let mut moved = Vec::new();
        let selected_before_insert = selected
            .iter()
            .filter(|key| key.track_index == to_track && key.item_index < insert_index)
            .count();
        self.push_timeline_undo();
        for (track_index, track) in self.timeline_tracks.iter_mut().enumerate() {
            let mut kept = Vec::with_capacity(track.items.len());
            for (item_index, item) in track.items.drain(..).enumerate() {
                if selected.contains(&TimelineSelectionKey {
                    track_index,
                    item_index,
                }) {
                    moved.push(item);
                } else {
                    kept.push(item);
                }
            }
            track.items = kept;
        }
        if moved.is_empty() {
            return;
        }
        let insert_index = insert_index
            .saturating_sub(selected_before_insert)
            .min(self.timeline_tracks[to_track].items.len());
        for (offset, item) in moved.iter().copied().enumerate() {
            self.timeline_tracks[to_track]
                .items
                .insert(insert_index + offset, item);
        }
        self.edit_selection.clear();
        for offset in 0..moved.len() {
            self.edit_selection.insert(TimelineSelectionKey {
                track_index: to_track,
                item_index: insert_index + offset,
            });
        }
        self.selection_anchor = Some(TimelineSelectionKey {
            track_index: to_track,
            item_index: insert_index,
        });
        self.status = format!(
            "Moved {} selected GOP{} to track {}",
            moved.len(),
            if moved.len() == 1 { "" } else { "s" },
            to_track + 1
        );
        if let Some(first) = moved.first().copied() {
            self.seek_item_start(first, ctx);
        }
    }

    pub(crate) fn clear(&mut self) {
        self.sources.clear();
        self.timeline_tracks.clear();
        self.active_source = None;
        self.clear_active_source_state();
        self.export_task = None;
        self.export_status.clear();
        self.playback_active = false;
        self.playback_last_tick = None;
        self.pending_probes = 0;
        self.total_probes = 0;
        self.probe_progress.clear();
        self.probe_replace_existing = false;
        self.pending_timecode_apply = None;
        self.timecode_apply_queue.clear();
        self.timeline_clipboard = None;
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.status = self.text().ready().to_owned();
    }

    pub(crate) fn start_selected_gop_export(&mut self, kind: ExportKind, ctx: &egui::Context) {
        if self.export_task.is_some() {
            return;
        }

        let Some(plan) = self.main_track_export_plan() else {
            self.export_status = self.text().main_track_empty().to_owned();
            return;
        };
        let export_options = match self.export_options_for(kind, &plan) {
            Ok(options) => options,
            Err(error) => {
                self.export_status = error;
                return;
            }
        };

        let default_name = match kind {
            ExportKind::Mp4 => {
                format!(
                    "main_track_{:04}_{:04}.mp4",
                    plan.first_gop + 1,
                    plan.last_gop + 1
                )
            }
            ExportKind::Raw => {
                let ext = match plan.codec_name.as_str() {
                    "h264" => "h264",
                    "hevc" => "h265",
                    _ => "bin",
                };
                format!(
                    "main_track_{:04}_{:04}.{ext}",
                    plan.first_gop + 1,
                    plan.last_gop + 1
                )
            }
        };
        let dialog = match kind {
            ExportKind::Mp4 => rfd::FileDialog::new().add_filter("MP4", &["mp4"]),
            ExportKind::Raw => {
                rfd::FileDialog::new().add_filter("Raw video", &["h264", "h265", "hevc", "bin"])
            }
        };
        let Some(output_path) = dialog.set_file_name(default_name).save_file() else {
            return;
        };

        let (sender, receiver) = mpsc::channel();
        let first_index = plan.first_gop;
        let gop_count = plan.segments.len();
        let segments = plan.segments;
        thread::spawn(move || {
            let started_at = Instant::now();
            let result =
                ffmpeg::export_video_segments(&segments, &output_path, kind, export_options);
            let _ = sender.send(ExportMessage::Done {
                kind,
                gop_index: first_index,
                gop_count,
                result,
                elapsed: started_at.elapsed(),
            });
        });

        self.export_task = Some(receiver);
        self.export_status = if gop_count == 1 {
            format!(
                "Exporting main track GOP {} as {}",
                first_index + 1,
                export_kind_label(kind)
            )
        } else {
            format!(
                "Exporting main track {gop_count} GOPs as {}",
                export_kind_label(kind)
            )
        };
        ctx.request_repaint();
    }

    fn main_track_export_plan(&self) -> Option<MainTrackExportPlan> {
        let main_track = self.timeline_tracks.first()?;
        let first_item = main_track.items.first().copied()?;
        let first_source = self.sources.get(first_item.source_index)?;
        let source_time_base = valid_time_base(&first_source.probe);
        let mut time_base_consistent = source_time_base.is_some();
        let segments = main_track
            .items
            .iter()
            .filter_map(|item| {
                let source = self.sources.get(item.source_index)?;
                if Some((source.probe.time_base_num, source.probe.time_base_den))
                    != source_time_base
                {
                    time_base_consistent = false;
                }
                let gop = source.probe.gops.get(item.gop_index)?;
                Some(ExportSegment {
                    input_path: source.path.clone(),
                    range: PacketRange {
                        start_packet: gop.start_frame,
                        end_packet: gop.end_frame,
                    },
                })
            })
            .collect::<Vec<_>>();
        let last_item = main_track.items.last().copied().unwrap_or(first_item);
        Some(MainTrackExportPlan {
            segments,
            first_gop: first_item.gop_index,
            last_gop: last_item.gop_index,
            codec_name: first_source.probe.codec_name.clone(),
            source_time_base,
            time_base_consistent,
        })
    }

    pub(crate) fn main_track_time_base_status(&self) -> Option<((i32, i32), bool)> {
        let plan = self.main_track_export_plan()?;
        Some((plan.source_time_base?, plan.time_base_consistent))
    }

    fn export_options_for(
        &self,
        kind: ExportKind,
        plan: &MainTrackExportPlan,
    ) -> Result<ExportOptions, String> {
        if kind == ExportKind::Raw {
            return Ok(ExportOptions::default());
        }
        let time_base = if self.export_use_source_time_base {
            if !plan.time_base_consistent {
                return Err(
                    "Main track source timebases differ; set a custom MP4 timebase".to_owned(),
                );
            }
            plan.source_time_base
                .ok_or_else(|| "Source timebase is unavailable".to_owned())?
        } else if self.export_time_base_num > 0 && self.export_time_base_den > 0 {
            (self.export_time_base_num, self.export_time_base_den)
        } else {
            return Err(
                "Custom MP4 timebase must use positive numerator and denominator".to_owned(),
            );
        };
        if self.export_generate_pts && self.export_pts_step < 0 {
            return Err("PTS step must be zero or a positive integer".to_owned());
        }
        Ok(ExportOptions {
            include_audio: self.export_include_audio,
            time_base: Some(time_base),
            generate_pts: self.export_generate_pts,
            pts_step: if self.export_generate_pts {
                self.export_pts_step
            } else {
                0
            },
        })
    }

    pub(crate) fn poll_export_task(&mut self, ctx: &egui::Context) {
        let Some(receiver) = &self.export_task else {
            return;
        };

        match receiver.try_recv() {
            Ok(ExportMessage::Done {
                kind,
                gop_index,
                gop_count,
                result,
                elapsed,
            }) => {
                self.export_task = None;
                match result {
                    Ok(summary) => {
                        self.export_status = format!(
                            "Exported {} {}: {} packets, {} bytes, {:.0} ms",
                            if gop_count == 1 {
                                format!("GOP {}", gop_index + 1)
                            } else {
                                format!("{gop_count} GOPs")
                            },
                            export_kind_label(kind),
                            summary.packets_written,
                            summary.bytes_written,
                            elapsed.as_secs_f64() * 1000.0
                        );
                    }
                    Err(error) => {
                        self.export_status = format!("Export failed: {error}");
                    }
                }
                ctx.request_repaint();
            }
            Err(mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(Duration::from_millis(100));
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.export_task = None;
                self.export_status = "Export worker disconnected".to_owned();
            }
        }
    }

    pub(crate) fn set_current_frame(&mut self, frame: i64, ctx: &egui::Context) {
        let Some(max_frame) = self.max_frame() else {
            return;
        };
        let frame = frame.clamp(0, max_frame);
        self.current_frame = Some(frame);
        if let Some(probe) = &self.probe {
            self.selected_gop = find_gop_index(probe, frame);
        }
        self.ensure_current_frame_cached(ctx);
    }

    pub(crate) fn select_gop(&mut self, selected: Option<usize>, ctx: &egui::Context) {
        self.selected_gop = selected;
        if let Some(frame) = selected
            .and_then(|index| self.probe.as_ref()?.gops.get(index))
            .map(|gop| gop.start_frame)
        {
            self.current_frame = Some(frame);
            self.ensure_current_frame_cached(ctx);
        }
    }

    pub(crate) fn select_edit_range(
        &mut self,
        first: TimelineSelectionKey,
        last: TimelineSelectionKey,
    ) {
        if first.track_index != last.track_index {
            self.edit_selection.insert(last);
            self.selection_anchor = Some(last);
            self.status = "Selected 1 GOP for editing".to_owned();
            return;
        }
        let Some(track) = self.timeline_tracks.get(first.track_index) else {
            return;
        };
        if track.items.is_empty() {
            return;
        }
        let max = track.items.len().saturating_sub(1);
        let first_item = first.item_index.min(max);
        let last_item = last.item_index.min(max);
        let (start, end) = if first_item <= last_item {
            (first_item, last_item)
        } else {
            (last_item, first_item)
        };
        for index in start..=end {
            self.edit_selection.insert(TimelineSelectionKey {
                track_index: first.track_index,
                item_index: index,
            });
        }
        self.selection_anchor = Some(first);
        self.status = format!(
            "Selected timeline positions {}-{} on track {}",
            start + 1,
            end + 1,
            first.track_index + 1
        );
    }

    pub(crate) fn handle_timeline_gop_click(
        &mut self,
        key: TimelineSelectionKey,
        source_index: usize,
        gop_index: usize,
        frame: i64,
        modifiers: egui::Modifiers,
        ctx: &egui::Context,
    ) {
        self.seek_source_frame(source_index, frame, ctx);
        if modifiers.command {
            if !self.edit_selection.remove(&key) {
                self.edit_selection.insert(key);
            }
            self.selection_anchor = Some(key);
            self.status = format!("Toggled GOP {} in edit selection", gop_index + 1);
        } else if modifiers.shift {
            let anchor = self
                .selection_anchor
                .filter(|anchor| anchor.track_index == key.track_index)
                .unwrap_or(key);
            self.select_edit_range(anchor, key);
        } else {
            self.edit_selection.clear();
            self.selection_anchor = Some(key);
        }
    }

    pub(crate) fn max_frame(&self) -> Option<i64> {
        self.probe
            .as_ref()
            .map(|probe| probe.frame_count.saturating_sub(1).max(0))
    }
}

fn video_export_compatible(first: &ProbeSummary, other: &ProbeSummary) -> bool {
    first.codec_id == other.codec_id
        && first.width == other.width
        && first.height == other.height
        && first.time_base_num == other.time_base_num
        && first.time_base_den == other.time_base_den
        && first.real_frame_rate_num == other.real_frame_rate_num
        && first.real_frame_rate_den == other.real_frame_rate_den
}

fn valid_time_base(probe: &ProbeSummary) -> Option<(i32, i32)> {
    if probe.time_base_num > 0 && probe.time_base_den > 0 {
        Some((probe.time_base_num, probe.time_base_den))
    } else {
        None
    }
}
