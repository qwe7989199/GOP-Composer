use std::{
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

use eframe::egui;

use crate::{
    app::{GopComposerApp, ProbeMessage, ProbeProgress, SourceEntry, TimelineTrack},
    media::ffmpeg,
    ui::i18n::ProbeProgressText,
    util::truncate_middle,
};

impl GopComposerApp {
    pub(crate) fn open_videos(&mut self, replace_existing: bool) {
        if self.task.is_some() {
            self.status = self.text().wait_indexing_import().to_owned();
            return;
        }
        let dialog = rfd::FileDialog::new().add_filter(
            "Video",
            &[
                "mp4", "mkv", "mov", "m4v", "ts", "m2ts", "h264", "264", "hevc", "h265",
            ],
        );
        let paths = if replace_existing {
            let Some(path) = dialog.pick_file() else {
                return;
            };
            vec![path]
        } else {
            let Some(paths) = dialog.pick_files() else {
                return;
            };
            paths
        };
        if paths.is_empty() {
            return;
        }

        if replace_existing {
            self.clear();
        }
        self.probe_replace_existing = replace_existing;

        let mut existing_tracks_added = 0usize;
        let mut paths_to_check = Vec::new();
        for path in paths {
            if let Some(source_index) = self.source_index_for_path(&path) {
                if self.append_track_for_source(source_index) {
                    existing_tracks_added += 1;
                    if !replace_existing {
                        self.queue_timecode_apply_for_source(source_index);
                    }
                }
            } else {
                paths_to_check.push(path);
            }
        }

        self.probe_progress.clear();
        let mut rejected = 0usize;
        let mut paths_to_probe = Vec::new();
        for path in paths_to_check {
            self.probe_progress
                .insert(path.clone(), ProbeProgress::pending(&path));
            match ffmpeg::probe_video_stream(&path) {
                Ok(summary) => {
                    if let Some(error) = unsupported_video_codec_message(&summary) {
                        self.set_probe_progress_error(&path, error.clone());
                        self.status = error;
                        rejected += 1;
                    } else {
                        paths_to_probe.push(path);
                    }
                }
                Err(error) => {
                    self.set_probe_progress_error(&path, error.clone());
                    self.status = self.text().probe_failed(&error);
                    eprintln!("Probe failed for {}: {error}", path.display());
                    rejected += 1;
                }
            }
        }

        if paths_to_probe.is_empty() {
            if existing_tracks_added > 0 && rejected == 0 {
                self.status = self.text().added_existing_tracks(existing_tracks_added);
            } else if existing_tracks_added > 0 {
                self.status = self
                    .text()
                    .added_existing_tracks_rejected(existing_tracks_added, rejected);
            }
            self.probe_replace_existing = false;
            return;
        }

        let (sender, receiver) = mpsc::channel();
        let pending = paths_to_probe.len();
        for path in paths_to_probe {
            let worker_path = path.clone();
            let worker_sender = sender.clone();
            thread::spawn(move || {
                let progress_path = worker_path.clone();
                let progress_sender = worker_sender.clone();
                let result = ffmpeg::probe_gops_with_progress(&worker_path, move |read, total| {
                    let _ = progress_sender.send(ProbeMessage::Progress {
                        path: progress_path.clone(),
                        bytes_read: read,
                        total_bytes: total,
                    });
                });
                let _ = worker_sender.send(ProbeMessage::Done {
                    path: worker_path,
                    result,
                });
            });
        }

        self.task = Some(receiver);
        self.task_started_at = Some(Instant::now());
        self.pending_probes = pending;
        self.total_probes = pending;
        self.status = self
            .text()
            .indexing_files(pending, existing_tracks_added, rejected);
    }

    pub(crate) fn poll_probe_task(&mut self, ctx: &egui::Context) {
        let Some(receiver) = self.task.take() else {
            return;
        };

        let mut keep_task = true;
        loop {
            match receiver.try_recv() {
                Ok(ProbeMessage::Progress {
                    path,
                    bytes_read,
                    total_bytes,
                }) => {
                    self.update_probe_progress(&path, bytes_read, total_bytes);
                }
                Ok(ProbeMessage::Done {
                    path,
                    result: Ok(probe),
                }) => {
                    self.finish_probe_success(path, probe, ctx);
                    if self.pending_probes == 0 {
                        break;
                    }
                }
                Ok(ProbeMessage::Done {
                    path,
                    result: Err(error),
                }) => {
                    self.finish_probe_error(path, error);
                    if self.pending_probes == 0 {
                        break;
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    if self.pending_probes > 0 {
                        self.status = self.text().probe_worker_disconnected().to_owned();
                    }
                    self.task_started_at = None;
                    self.total_probes = 0;
                    self.probe_replace_existing = false;
                    keep_task = false;
                    break;
                }
            }
        }

        if self.pending_probes == 0 {
            self.task_started_at = None;
            self.total_probes = 0;
            self.probe_replace_existing = false;
            keep_task = false;
        }

        if keep_task {
            self.task = Some(receiver);
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }

    pub(crate) fn probe_progress_ui(&self, ui: &mut egui::Ui) {
        let elapsed = self
            .task_started_at
            .map(|time| time.elapsed().as_secs_f32())
            .unwrap_or(0.0);
        let completed = self
            .probe_progress
            .values()
            .filter(|item| item.done)
            .count();
        let total = self.probe_progress.len();
        let (bytes_read, total_bytes) = self.aggregate_probe_bytes();
        let known_total = total_bytes > 0;
        let finalizing = self
            .probe_progress
            .values()
            .filter(|item| {
                !item.done && item.total_bytes > 0 && item.bytes_read >= item.total_bytes
            })
            .count();
        let progress = if known_total {
            (bytes_read as f32 / total_bytes as f32).clamp(0.0, 1.0)
        } else {
            0.0
        };

        let text = if known_total {
            self.text().probe_progress(ProbeProgressText {
                completed,
                total,
                pending: self.pending_probes,
                finalizing,
                bytes_read: &format_bytes(bytes_read),
                total_bytes: &format_bytes(total_bytes),
                percent: progress * 100.0,
                elapsed,
            })
        } else {
            self.text()
                .probe_progress_unknown(completed, total, self.pending_probes, elapsed)
        };
        ui.add(
            egui::ProgressBar::new(progress)
                .animate(!known_total)
                .corner_radius(egui::CornerRadius::ZERO)
                .desired_width(ui.available_width())
                .text(text),
        );

        egui::Grid::new("probe_file_progress")
            .num_columns(3)
            .spacing([8.0, 2.0])
            .striped(false)
            .show(ui, |ui| {
                for item in self.probe_progress.values() {
                    ui.label(truncate_middle(&item.name, 34));
                    let known = item.total_bytes > 0;
                    let file_progress = if known {
                        (item.bytes_read as f32 / item.total_bytes as f32).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    let label = if let Some(error) = &item.error {
                        self.text().probe_file_failed(&truncate_middle(error, 48))
                    } else if item.done {
                        self.text().done().to_owned()
                    } else if known && item.bytes_read >= item.total_bytes {
                        self.text().finalizing_index(
                            &format_bytes(item.bytes_read),
                            &format_bytes(item.total_bytes),
                        )
                    } else if known {
                        format!(
                            "{} / {} · {:.2}%",
                            format_bytes(item.bytes_read),
                            format_bytes(item.total_bytes),
                            file_progress * 100.0
                        )
                    } else {
                        self.text()
                            .read_size_unknown(&format_bytes(item.bytes_read))
                    };
                    ui.add(
                        egui::ProgressBar::new(file_progress)
                            .animate(!known && !item.done)
                            .corner_radius(egui::CornerRadius::ZERO)
                            .desired_width(220.0)
                            .text(label),
                    );
                    ui.end_row();
                }
            });
    }

    fn update_probe_progress(&mut self, path: &Path, bytes_read: i64, total_bytes: i64) {
        let entry = self
            .probe_progress
            .entry(path.to_path_buf())
            .or_insert_with(|| ProbeProgress::pending(path));
        entry.bytes_read = bytes_read.max(entry.bytes_read).max(0);
        if total_bytes > 0 {
            entry.total_bytes = total_bytes;
            entry.bytes_read = entry.bytes_read.min(total_bytes);
        }
    }

    fn finish_probe_success(
        &mut self,
        path: PathBuf,
        probe: ffmpeg::ProbeSummary,
        ctx: &egui::Context,
    ) {
        if let Some(progress) = self.probe_progress.get_mut(&path) {
            if progress.total_bytes > 0 {
                progress.bytes_read = progress.total_bytes;
            }
            progress.done = true;
        }

        let elapsed = self
            .task_started_at
            .map(|time| time.elapsed().as_secs_f32())
            .unwrap_or(0.0);
        if let Some(error) = unsupported_video_codec_message(&probe) {
            self.finish_probe_unsupported_codec(path, error);
            return;
        }
        let (source_index, source_added) =
            if let Some(source_index) = self.source_index_for_path(&path) {
                self.append_track_for_source(source_index);
                (source_index, false)
            } else {
                let source_index = self.sources.len();
                let source = SourceEntry::new(path, probe, source_index);
                self.sources.push(source);
                self.append_track_for_source(source_index);
                (source_index, true)
            };
        self.pending_probes = self.pending_probes.saturating_sub(1);
        if self.active_source.is_none() {
            self.activate_source(source_index, ctx);
        }
        self.status = self.text().indexed_sources_tracks_pending(
            source_added,
            self.sources.len(),
            self.timeline_tracks.len(),
            self.pending_probes,
            elapsed,
        );
        if !self.probe_replace_existing {
            self.queue_timecode_apply_for_source(source_index);
        }
    }

    fn finish_probe_unsupported_codec(&mut self, path: PathBuf, error: String) {
        self.status = error.clone();
        self.set_probe_progress_error(&path, error);
        self.pending_probes = self.pending_probes.saturating_sub(1);
    }

    fn set_probe_progress_error(&mut self, path: &Path, error: String) {
        let entry = self
            .probe_progress
            .entry(path.to_path_buf())
            .or_insert_with(|| ProbeProgress::pending(path));
        if entry.total_bytes > 0 {
            entry.bytes_read = entry.total_bytes;
        }
        entry.done = true;
        entry.error = Some(error);
    }

    fn finish_probe_error(&mut self, path: PathBuf, error: String) {
        self.status = self.text().probe_failed(&error);
        eprintln!("Probe failed for {}: {error}", path.display());
        self.set_probe_progress_error(&path, error);
        self.pending_probes = self.pending_probes.saturating_sub(1);
    }

    fn aggregate_probe_bytes(&self) -> (i64, i64) {
        self.probe_progress.values().fold((0, 0), |acc, item| {
            if item.total_bytes > 0 {
                (
                    acc.0 + item.bytes_read.clamp(0, item.total_bytes),
                    acc.1 + item.total_bytes,
                )
            } else {
                acc
            }
        })
    }

    fn source_index_for_path(&self, path: &Path) -> Option<usize> {
        let key = source_path_key(path);
        self.sources
            .iter()
            .position(|source| source_path_key(&source.path) == key)
    }

    fn append_track_for_source(&mut self, source_index: usize) -> bool {
        let Some(source) = self.sources.get(source_index) else {
            return false;
        };
        let track = TimelineTrack::from_source(source, source_index);
        self.timeline_tracks.push(track);
        true
    }
}

impl ProbeProgress {
    fn pending(path: &Path) -> Self {
        let total_bytes = path
            .metadata()
            .ok()
            .and_then(|metadata| i64::try_from(metadata.len()).ok())
            .unwrap_or(0);
        Self {
            name: path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("source")
                .to_owned(),
            bytes_read: 0,
            total_bytes,
            done: false,
            error: None,
        }
    }
}

fn source_path_key(path: &Path) -> String {
    let normalized = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let key = normalized.to_string_lossy().replace('\\', "/");
    if cfg!(target_os = "windows") {
        key.to_ascii_lowercase()
    } else {
        key
    }
}

fn unsupported_video_codec_message(probe: &ffmpeg::ProbeSummary) -> Option<String> {
    let codec = probe.codec_name.to_ascii_lowercase();
    if matches!(
        codec.as_str(),
        "h264" | "avc" | "avc1" | "hevc" | "h265" | "hvc1" | "hev1"
    ) {
        None
    } else {
        Some(format!(
            "Unsupported video codec: {}. GOP Composer currently supports H.264/AVC and H.265/HEVC only.",
            probe.codec_name
        ))
    }
}

fn format_bytes(bytes: i64) -> String {
    let bytes = bytes.max(0) as f64;
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    const GIB: f64 = MIB * 1024.0;
    if bytes >= GIB {
        format!("{:.2} GiB", bytes / GIB)
    } else if bytes >= MIB {
        format!("{:.1} MiB", bytes / MIB)
    } else if bytes >= KIB {
        format!("{:.1} KiB", bytes / KIB)
    } else {
        format!("{} B", bytes as i64)
    }
}
