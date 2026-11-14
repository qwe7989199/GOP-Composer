use std::{
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

use eframe::egui;

use super::{ExportMessage, GopComposerApp, MainTrackExportPlan};
use crate::{
    media::ffmpeg::{self, ExportKind, ExportOptions, ExportSegment, PacketRange, ProbeSummary},
    util::export_kind_label,
};

impl GopComposerApp {
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
}

fn valid_time_base(probe: &ProbeSummary) -> Option<(i32, i32)> {
    if probe.time_base_num > 0 && probe.time_base_den > 0 {
        Some((probe.time_base_num, probe.time_base_den))
    } else {
        None
    }
}
