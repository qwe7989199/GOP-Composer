use std::{sync::mpsc, time::Duration};

use eframe::egui;

use crate::{
    app::{GopComposerApp, TimelineSelectionKey},
    media::ffmpeg::{DecodedFrame, ProbeSummary},
    preview::{
        gl::{self as gl_preview, GlPreviewState},
        worker::{PreviewMessage, PreviewRequest, PreviewRequestKind},
    },
    ui::panels::preview_panel,
    util::anchor_gop_for_frame,
};

const PREVIEW_PREFETCH_FORWARD: i64 = 24;
const PREVIEW_CACHE_LIMIT: usize = 48;
const PREVIEW_PENDING_DECODED_LIMIT: usize = 48;
const PREVIEW_TEXTURE_UPLOADS_PER_TICK: usize = 2;

impl GopComposerApp {
    pub(crate) fn draw_preview_panel(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
    ) -> Option<egui::Rect> {
        let (Some(path), Some(probe), Some(current_frame)) = (
            self.selected_path.clone(),
            self.probe.clone(),
            self.current_frame,
        ) else {
            return Some(preview_panel(ui, self.preview_texture(), self.text()));
        };

        if self.gl_preview.should_prefer_native()
            && let Some(request) = GlPreviewState::request_for(&path, &probe, current_frame)
        {
            let available = ui.available_size();
            let desired = egui::vec2(available.x.max(1.0), available.y.max(1.0));
            let (rect, _) = ui.allocate_exact_size(desired, egui::Sense::hover());
            ui.painter()
                .rect_filled(rect, 0.0, ui.visuals().extreme_bg_color);
            let image_rect = gl_preview::fit_rect(rect, request.width, request.height);
            self.gl_preview.paint(ui, image_rect, request);
            let status = self.gl_preview.last_status();
            if status.starts_with("GL preview disabled") {
                self.preview_status = status;
            } else if let Some(rendered) = self.gl_preview.rendered_frame(&path) {
                self.preview_status = format!("GL/D3D preview frame {rendered}");
            } else {
                self.preview_status = format!("GL/D3D preview frame {current_frame}");
            }
            if !self.gl_preview.is_rendered(&path, current_frame) {
                ctx.request_repaint_after(Duration::from_millis(16));
            }
            self.preload_next_gl_preview(current_frame);
            return Some(rect);
        }

        self.ensure_current_frame_cached(ctx);
        Some(preview_panel(ui, self.preview_texture(), self.text()))
    }

    pub(crate) fn ensure_current_frame_cached(&mut self, ctx: &egui::Context) {
        let Some(current_frame) = self.current_frame else {
            return;
        };
        if self.gl_preview.should_prefer_native() {
            self.preview_pending_frame = None;
            return;
        }
        if self.current_frame_preview_ready(ctx) {
            self.preview_display_frame = Some(current_frame);
            self.preview_status = format!("Preview frame {} cached", current_frame);
            if self.preview_pending_frame == Some(current_frame) {
                self.preview_pending_frame = None;
            }
            self.maybe_prefetch_preview(current_frame, ctx);
            return;
        }
        if self.preview_request.is_some() {
            self.preview_pending_frame = Some(current_frame);
            self.preview_status = format!("Waiting for decoder, target frame {}", current_frame);
        } else if self.preview_worker.is_none() {
            self.preview_status = "Preview worker is not ready".to_owned();
            return;
        }

        let kind = if self.playback_active {
            PreviewRequestKind::Prefetch
        } else {
            PreviewRequestKind::Interactive
        };
        if let Some(request) = self.build_preview_request(current_frame, kind) {
            self.start_preview_request(request, ctx);
        }
    }

    fn maybe_prefetch_preview(&mut self, current_frame: i64, ctx: &egui::Context) {
        if !self.playback_active && frame_navigation_key_down(ctx) {
            return;
        }
        if self.preview_request.is_some() || self.preview_worker.is_none() {
            return;
        }
        let Some(max_frame) = self.max_frame() else {
            return;
        };
        let start_frame = (current_frame + 1).min(max_frame);
        if start_frame <= current_frame {
            return;
        }
        let end_frame = (start_frame + PREVIEW_PREFETCH_FORWARD - 1).min(max_frame);
        if (start_frame..=end_frame).all(|frame| self.has_preview_frame(frame)) {
            return;
        }

        let Some(path) = self.selected_path.clone() else {
            return;
        };
        let Some(probe) = self.probe.as_ref() else {
            return;
        };
        let Some(anchor) = anchor_gop_for_frame(probe, start_frame) else {
            return;
        };
        let request = PreviewRequest {
            path,
            anchor_frame: anchor.start_frame,
            anchor_pts: anchor.pts,
            start_frame,
            frame_count: (end_frame - start_frame + 1) as i32,
            kind: PreviewRequestKind::Prefetch,
        };
        if request.frame_count > 0 {
            self.start_preview_request(request, ctx);
        }
    }

    fn build_preview_request(
        &self,
        target_frame: i64,
        kind: PreviewRequestKind,
    ) -> Option<PreviewRequest> {
        let path = self.selected_path.clone()?;
        let probe = self.probe.as_ref()?;
        let max_frame = self.max_frame()?;
        let start_frame = match kind {
            PreviewRequestKind::Interactive => target_frame,
            PreviewRequestKind::Prefetch => target_frame,
        };
        let frame_count = match kind {
            PreviewRequestKind::Interactive => 1,
            PreviewRequestKind::Prefetch => PREVIEW_PREFETCH_FORWARD,
        }
        .min(max_frame - start_frame + 1)
        .max(1) as i32;
        let anchor = anchor_gop_for_frame(probe, start_frame)?;
        Some(PreviewRequest {
            path,
            anchor_frame: anchor.start_frame,
            anchor_pts: anchor.pts,
            start_frame,
            frame_count,
            kind,
        })
    }

    fn start_preview_request(&mut self, request: PreviewRequest, ctx: &egui::Context) {
        let Some(worker) = &self.preview_worker else {
            self.preview_status = "Preview worker is not ready".to_owned();
            return;
        };

        if worker.requests.send(request.clone()).is_err() {
            self.preview_status = "Preview worker disconnected".to_owned();
            self.preview_worker = None;
            return;
        }
        self.preview_request = Some(request.clone());
        if request.kind == PreviewRequestKind::Interactive
            && self.preview_pending_frame != Some(request.start_frame)
        {
            self.preview_pending_frame = None;
        }
        if request.kind == PreviewRequestKind::Interactive {
            self.preview_status = format!(
                "Decoding frames {}-{} from GOP frame {}",
                request.start_frame,
                request.start_frame + i64::from(request.frame_count) - 1,
                request.anchor_frame
            );
        }
        ctx.request_repaint();
    }

    pub(crate) fn poll_preview_task(&mut self, ctx: &egui::Context) {
        if self.upload_queued_preview_frames(ctx, PREVIEW_TEXTURE_UPLOADS_PER_TICK) > 0 {
            ctx.request_repaint();
        }

        let Some(worker) = &self.preview_worker else {
            return;
        };

        match worker.responses.try_recv() {
            Ok(PreviewMessage::Ready { accel }) => {
                self.preview_status = format!("Preview decoder: {accel}");
                ctx.request_repaint();
            }
            Ok(PreviewMessage::Done {
                request,
                result,
                elapsed,
            }) => {
                let is_current_request = self.preview_request.as_ref() == Some(&request);
                match result {
                    Ok(frames) => {
                        let count = frames.len();
                        let first_frame = frames.first().map(|frame| frame.frame_index);
                        let last_frame = frames.last().map(|frame| frame.frame_index);
                        let current_frame = self.current_frame;
                        for decoded in frames {
                            if current_frame == Some(decoded.frame_index)
                                || self.preview_display_frame.is_none()
                            {
                                self.cache_decoded_frame(ctx, decoded);
                            } else {
                                self.preview_pending_decoded.push_back(decoded);
                            }
                        }
                        self.upload_queued_preview_frames(ctx, PREVIEW_TEXTURE_UPLOADS_PER_TICK);
                        if let Some(current) = self.current_frame
                            && self.preview_cache.contains_key(&current)
                        {
                            self.preview_display_frame = Some(current);
                        }
                        self.prune_preview_cache();
                        self.prune_pending_decoded();
                        if is_current_request && request.kind == PreviewRequestKind::Interactive {
                            let current = self.current_frame.unwrap_or(request.start_frame);
                            self.preview_status = format!(
                                "Decoded {} frames in {:.0} ms, current {}{}",
                                count,
                                elapsed.as_secs_f64() * 1000.0,
                                current,
                                if self.preview_cache.contains_key(&current) {
                                    ""
                                } else {
                                    " not cached"
                                }
                            );
                            if let (Some(first), Some(last), Some(pending)) =
                                (first_frame, last_frame, self.preview_pending_frame)
                                && (pending < first || pending > last)
                            {
                                self.preview_status =
                                    format!("Decoded stale window, scheduling frame {pending}");
                            }
                        }
                    }
                    Err(error) => {
                        if is_current_request && request.kind == PreviewRequestKind::Interactive {
                            self.preview_status = format!("Preview failed: {error}");
                        }
                    }
                }
                if is_current_request {
                    self.preview_request = None;
                    if let Some(pending) = self.preview_pending_frame.take() {
                        if self.current_frame == Some(pending)
                            && !self.preview_cache.contains_key(&pending)
                        {
                            self.ensure_current_frame_cached(ctx);
                        } else if let Some(current) = self.current_frame {
                            self.maybe_prefetch_preview(current, ctx);
                        }
                    } else if request.kind == PreviewRequestKind::Interactive {
                        self.ensure_current_frame_cached(ctx);
                    } else if let Some(current) = self.current_frame {
                        self.maybe_prefetch_preview(current, ctx);
                    }
                }
                ctx.request_repaint();
            }
            Ok(PreviewMessage::Failed(error)) => {
                self.preview_worker = None;
                self.preview_request = None;
                self.preview_pending_frame = None;
                self.preview_status = format!("Preview worker failed: {error}");
                ctx.request_repaint();
            }
            Err(mpsc::TryRecvError::Empty) => {
                if self.preview_request.is_some() {
                    ctx.request_repaint_after(Duration::from_millis(16));
                }
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.preview_worker = None;
                self.preview_request = None;
                self.preview_status = "Preview worker disconnected".to_owned();
                self.preview_pending_frame = None;
            }
        }
    }

    fn cache_decoded_frame(&mut self, ctx: &egui::Context, decoded: DecodedFrame) {
        let frame_index = decoded.frame_index;
        let image = egui::ColorImage::from_rgba_unmultiplied(
            [decoded.frame.width, decoded.frame.height],
            &decoded.frame.pixels,
        );
        let texture = ctx.load_texture(
            format!("preview-{}", decoded.frame_index),
            image,
            egui::TextureOptions::LINEAR,
        );
        self.preview_cache.insert(frame_index, texture);
        if self.current_frame == Some(frame_index) || self.preview_display_frame.is_none() {
            self.preview_display_frame = Some(frame_index);
        }
    }

    pub(crate) fn current_frame_preview_ready(&mut self, ctx: &egui::Context) -> bool {
        let Some(frame) = self.current_frame else {
            return false;
        };
        if self.preview_cache.contains_key(&frame) {
            return true;
        }
        self.promote_pending_decoded_frame(frame, ctx)
    }

    pub(crate) fn preview_texture(&self) -> Option<&egui::TextureHandle> {
        self.current_frame
            .and_then(|frame| self.preview_cache.get(&frame))
            .or_else(|| {
                self.preview_display_frame
                    .and_then(|frame| self.preview_cache.get(&frame))
            })
    }

    fn has_preview_frame(&self, frame: i64) -> bool {
        self.preview_cache.contains_key(&frame) || self.has_pending_decoded_frame(frame)
    }

    fn has_pending_decoded_frame(&self, frame: i64) -> bool {
        self.preview_pending_decoded
            .iter()
            .any(|decoded| decoded.frame_index == frame)
    }

    fn promote_pending_decoded_frame(&mut self, frame: i64, ctx: &egui::Context) -> bool {
        let Some(index) = self
            .preview_pending_decoded
            .iter()
            .position(|decoded| decoded.frame_index == frame)
        else {
            return false;
        };
        let Some(decoded) = self.preview_pending_decoded.remove(index) else {
            return false;
        };
        self.cache_decoded_frame(ctx, decoded);
        true
    }

    fn upload_queued_preview_frames(&mut self, ctx: &egui::Context, limit: usize) -> usize {
        let mut uploaded = 0usize;
        for _ in 0..limit {
            let Some(decoded) = self.preview_pending_decoded.pop_front() else {
                break;
            };
            self.cache_decoded_frame(ctx, decoded);
            uploaded += 1;
        }
        if uploaded > 0 {
            self.prune_preview_cache();
            self.prune_pending_decoded();
        }
        uploaded
    }

    fn prune_preview_cache(&mut self) {
        let center = self.current_frame.unwrap_or(0);
        while self.preview_cache.len() > PREVIEW_CACHE_LIMIT {
            let Some(remove_frame) = self
                .preview_cache
                .keys()
                .max_by_key(|frame| (*frame - center).abs())
                .copied()
            else {
                return;
            };
            self.preview_cache.remove(&remove_frame);
        }
    }

    fn prune_pending_decoded(&mut self) {
        let center = self.current_frame.unwrap_or(0);
        while self.preview_pending_decoded.len() > PREVIEW_PENDING_DECODED_LIMIT {
            let Some((remove_index, _)) = self
                .preview_pending_decoded
                .iter()
                .enumerate()
                .max_by_key(|(_, frame)| (frame.frame_index - center).abs())
            else {
                return;
            };
            self.preview_pending_decoded.remove(remove_index);
        }
    }

    fn preload_next_gl_preview(&mut self, current_frame: i64) {
        let Some((path, probe, target_frame)) = self.next_timeline_gop_start(current_frame) else {
            return;
        };
        let _ = self.gl_preview.preload(&path, &probe, target_frame);
    }

    fn next_timeline_gop_start(
        &self,
        current_frame: i64,
    ) -> Option<(std::path::PathBuf, ProbeSummary, i64)> {
        let source_index = self.active_source?;
        let current_key = self.current_timeline_key(source_index, current_frame)?;
        let next_item = self
            .timeline_tracks
            .get(current_key.track_index)?
            .items
            .get(current_key.item_index + 1)
            .copied()?;
        let source = self.sources.get(next_item.source_index)?;
        let target_frame = source.probe.gops.get(next_item.gop_index)?.start_frame;
        Some((source.path.clone(), source.probe.clone(), target_frame))
    }

    fn current_timeline_key(
        &self,
        source_index: usize,
        current_frame: i64,
    ) -> Option<TimelineSelectionKey> {
        if let Some(anchor) = self.selection_anchor
            && self.timeline_key_contains(anchor, source_index, current_frame)
        {
            return Some(anchor);
        }

        let mut fallback = None;
        for (track_index, track) in self.timeline_tracks.iter().enumerate() {
            for (item_index, item) in track.items.iter().enumerate() {
                if item.source_index != source_index {
                    continue;
                }
                let key = TimelineSelectionKey {
                    track_index,
                    item_index,
                };
                if !self.timeline_key_contains(key, source_index, current_frame) {
                    continue;
                }
                if self.selected_gop == Some(item.gop_index) {
                    return Some(key);
                }
                fallback.get_or_insert(key);
            }
        }
        fallback
    }

    fn timeline_key_contains(
        &self,
        key: TimelineSelectionKey,
        source_index: usize,
        current_frame: i64,
    ) -> bool {
        let Some(item) = self
            .timeline_tracks
            .get(key.track_index)
            .and_then(|track| track.items.get(key.item_index))
        else {
            return false;
        };
        if item.source_index != source_index {
            return false;
        }
        self.sources
            .get(item.source_index)
            .and_then(|source| source.probe.gops.get(item.gop_index))
            .is_some_and(|gop| current_frame >= gop.start_frame && current_frame <= gop.end_frame)
    }
}

fn frame_navigation_key_down(ctx: &egui::Context) -> bool {
    ctx.input(|input| {
        input.key_down(egui::Key::ArrowLeft)
            || input.key_down(egui::Key::ArrowRight)
            || input.key_down(egui::Key::ArrowUp)
            || input.key_down(egui::Key::ArrowDown)
    })
}
