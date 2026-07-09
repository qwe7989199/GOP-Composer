use std::time::{Duration, Instant};

use eframe::egui;

use crate::app::{GopComposerApp, TimelineItem, TimelineSelectionKey};

const FALLBACK_FPS: f64 = 24000.0 / 1001.0;

#[derive(Clone, Copy)]
struct PlaybackPosition {
    track_index: usize,
    item_index: usize,
    item: TimelineItem,
    gop_end: i64,
}

impl GopComposerApp {
    pub(crate) fn toggle_playback(&mut self, ctx: &egui::Context) {
        if self.playback_active {
            self.stop_playback();
        } else {
            self.start_playback(ctx);
        }
    }

    pub(crate) fn start_playback(&mut self, ctx: &egui::Context) {
        if self.active_source.is_none() || self.current_frame.is_none() {
            self.status = self.text().no_playhead_position().to_owned();
            return;
        }
        self.playback_active = true;
        self.playback_last_tick = Some(Instant::now());
        self.status = self.text().playback_started().to_owned();
        self.ensure_current_frame_cached(ctx);
        ctx.request_repaint();
    }

    pub(crate) fn stop_playback(&mut self) {
        self.playback_active = false;
        self.playback_last_tick = None;
    }

    pub(crate) fn tick_playback(&mut self, ctx: &egui::Context) {
        if !self.playback_active {
            return;
        }
        if !self.current_playback_frame_ready(ctx) {
            self.playback_last_tick = None;
            return;
        }

        let frame_duration = Duration::from_secs_f64(1.0 / self.playback_fps().max(1.0));
        let now = Instant::now();
        let Some(last) = self.playback_last_tick else {
            self.playback_last_tick = Some(now);
            ctx.request_repaint_after(frame_duration);
            return;
        };
        let elapsed = now.saturating_duration_since(last);
        if elapsed < frame_duration {
            ctx.request_repaint_after(frame_duration - elapsed);
            return;
        }

        if !self.advance_playback_one_frame(ctx) {
            self.stop_playback();
            self.status = self.text().playback_stopped_at_end().to_owned();
            ctx.request_repaint();
            return;
        }

        self.playback_last_tick = Some(now);
        ctx.request_repaint_after(frame_duration);
    }

    fn current_playback_frame_ready(&mut self, ctx: &egui::Context) -> bool {
        let Some(frame) = self.current_frame else {
            return false;
        };
        if self.gl_preview.should_prefer_native() {
            let Some(path) = self.selected_path.as_deref() else {
                return false;
            };
            if self.gl_preview.is_rendered(path, frame) {
                return true;
            }
            ctx.request_repaint_after(Duration::from_millis(2));
            return false;
        }
        if self.preview_cache.contains_key(&frame) {
            return true;
        }
        self.ensure_current_frame_cached(ctx);
        ctx.request_repaint_after(Duration::from_millis(8));
        false
    }

    fn advance_playback_one_frame(&mut self, ctx: &egui::Context) -> bool {
        let (Some(source_index), Some(frame)) = (self.active_source, self.current_frame) else {
            return false;
        };

        if let Some(position) = self.playback_position(source_index, frame) {
            if frame < position.gop_end {
                self.seek_source_frame(position.item.source_index, frame + 1, ctx);
                return true;
            }
            if let Some(next_item) = self
                .timeline_tracks
                .get(position.track_index)
                .and_then(|track| track.items.get(position.item_index + 1))
                .copied()
            {
                return self.seek_playback_item_start(next_item, ctx);
            }
            return false;
        }

        let Some(max_frame) = self.max_frame() else {
            return false;
        };
        if frame >= max_frame {
            return false;
        }
        self.set_current_frame(frame + 1, ctx);
        true
    }

    fn seek_playback_item_start(&mut self, item: TimelineItem, ctx: &egui::Context) -> bool {
        let Some(frame) = self
            .sources
            .get(item.source_index)
            .and_then(|source| source.probe.gops.get(item.gop_index))
            .map(|gop| gop.start_frame)
        else {
            return false;
        };
        self.seek_source_frame(item.source_index, frame, ctx);
        true
    }

    fn playback_position(&self, source_index: usize, frame: i64) -> Option<PlaybackPosition> {
        if let Some(anchor) = self.selection_anchor
            && let Some(position) = self.playback_position_for_key(anchor, source_index, frame)
        {
            return Some(position);
        }

        let selected_gop = self.selected_gop;
        let mut fallback = None;
        for (track_index, track) in self.timeline_tracks.iter().enumerate() {
            for (item_index, item) in track.items.iter().copied().enumerate() {
                if item.source_index != source_index {
                    continue;
                }
                let Some(gop) = self
                    .sources
                    .get(item.source_index)
                    .and_then(|source| source.probe.gops.get(item.gop_index))
                else {
                    continue;
                };
                if frame < gop.start_frame || frame > gop.end_frame {
                    continue;
                }
                let position = PlaybackPosition {
                    track_index,
                    item_index,
                    item,
                    gop_end: gop.end_frame,
                };
                if selected_gop == Some(item.gop_index) {
                    return Some(position);
                }
                fallback.get_or_insert(position);
            }
        }
        fallback
    }

    fn playback_position_for_key(
        &self,
        key: TimelineSelectionKey,
        source_index: usize,
        frame: i64,
    ) -> Option<PlaybackPosition> {
        let item = self
            .timeline_tracks
            .get(key.track_index)?
            .items
            .get(key.item_index)
            .copied()?;
        if item.source_index != source_index {
            return None;
        }
        let gop = self
            .sources
            .get(item.source_index)?
            .probe
            .gops
            .get(item.gop_index)?;
        if frame < gop.start_frame || frame > gop.end_frame {
            return None;
        }
        Some(PlaybackPosition {
            track_index: key.track_index,
            item_index: key.item_index,
            item,
            gop_end: gop.end_frame,
        })
    }

    fn playback_fps(&self) -> f64 {
        let Some(source) = self.active_source.and_then(|index| self.sources.get(index)) else {
            return FALLBACK_FPS;
        };
        rational_fps(
            source.probe.real_frame_rate_num,
            source.probe.real_frame_rate_den,
        )
        .or_else(|| {
            rational_fps(
                source.probe.avg_frame_rate_num,
                source.probe.avg_frame_rate_den,
            )
        })
        .unwrap_or(FALLBACK_FPS)
    }
}

fn rational_fps(num: i32, den: i32) -> Option<f64> {
    if num > 0 && den > 0 {
        Some(num as f64 / den as f64)
    } else {
        None
    }
}
