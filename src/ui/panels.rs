use std::collections::BTreeSet;

use eframe::egui;

use crate::{
    app::{SourceEntry, TimelineAction, TimelineSelectionKey, TimelineTrack},
    media::ffmpeg::{GopInfo, ProbeSummary},
    ui::i18n::Text,
    util::{
        choose_tick_step, format_bytes, mix_color, pastel_highlight, stream_details,
        truncate_middle,
    },
};

const TRACK_LABEL_WIDTH: f32 = 180.0;
const TRACK_HEIGHT: f32 = 44.0;
const TIMELINE_MIN_ZOOM: f32 = 0.05;
const TIMELINE_MAX_ZOOM: f32 = 24.0;
const INK: egui::Color32 = egui::Color32::from_rgb(42, 53, 68);
const UI_PANEL: egui::Color32 = egui::Color32::from_rgb(236, 246, 255);
const UI_PANEL_ALT: egui::Color32 = egui::Color32::from_rgb(225, 241, 255);
const UI_SURFACE: egui::Color32 = egui::Color32::from_rgb(248, 252, 255);
const UI_STROKE: egui::Color32 = egui::Color32::from_rgb(189, 211, 232);
const UI_ACCENT_SOFT: egui::Color32 = egui::Color32::from_rgb(92, 142, 192);
const UI_ACCENT_STRONG: egui::Color32 = egui::Color32::from_rgb(58, 103, 154);

struct DrawItem<'a> {
    source: &'a SourceEntry,
    gop: &'a GopInfo,
    source_index: usize,
    gop_index: usize,
    start_frame: i64,
    frame_count: i64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct DragKey {
    track_index: usize,
    item_index: usize,
    source_index: usize,
    gop_index: usize,
}

#[derive(Clone, Debug, Default)]
struct TimelineDragState {
    key: DragKey,
    previews: Vec<DragPreviewItem>,
    dragged_keys: BTreeSet<TimelineSelectionKey>,
}

#[derive(Clone, Debug)]
struct DragPreviewItem {
    offset: egui::Vec2,
    size: egui::Vec2,
    fill: egui::Color32,
    stroke: egui::Color32,
    label: usize,
}

pub(crate) struct TimelineView<'a> {
    pub(crate) sources: &'a [SourceEntry],
    pub(crate) tracks: &'a [TimelineTrack],
    pub(crate) active_source: Option<usize>,
    pub(crate) current_frame: Option<i64>,
    pub(crate) selected: Option<usize>,
    pub(crate) edit_selection: &'a BTreeSet<TimelineSelectionKey>,
    pub(crate) zoom: f32,
    pub(crate) text: Text,
}

pub(crate) fn empty_state(ui: &mut egui::Ui, text: Text) {
    ui.vertical_centered(|ui| {
        ui.add_space(120.0);
        ui.heading(text.open_video_empty_title());
        ui.label(text.open_video_empty_body());
    });
}

pub(crate) fn summary_grid(ui: &mut egui::Ui, probe: &ProbeSummary, text: Text) {
    egui::Grid::new("summary_grid")
        .num_columns(2)
        .spacing([12.0, 6.0])
        .striped(true)
        .show(ui, |ui| {
            row(
                ui,
                text.codec(),
                &format!("{} ({})", probe.codec_name, probe.codec_id),
            );
            row(
                ui,
                text.size(),
                &format!("{} x {}", probe.width, probe.height),
            );
            row(
                ui,
                text.timebase(),
                &format_rational(probe.time_base_num, probe.time_base_den),
            );
            row(
                ui,
                text.frame_rate(),
                &format_frame_rate(probe.real_frame_rate_num, probe.real_frame_rate_den),
            );
            row(
                ui,
                text.avg_rate(),
                &format_frame_rate(probe.avg_frame_rate_num, probe.avg_frame_rate_den),
            );
            row(
                ui,
                text.duration(),
                &format!("{:.3}s", probe.duration_seconds()),
            );
            row(ui, text.packets(), &probe.frame_count.to_string());
            row(ui, text.gops(), &probe.gops.len().to_string());
            if let Some(timecode) = &probe.timecode {
                row(
                    ui,
                    text.timecode(),
                    &format!(
                        "{} @ {}/{} frame {}",
                        timecode.value, timecode.rate_num, timecode.rate_den, timecode.start_frame
                    ),
                );
            }
        });
}

pub(crate) fn streams_grid(ui: &mut egui::Ui, probe: &ProbeSummary, text: Text) {
    section_heading(ui, text.streams());
    egui::Grid::new("streams_grid")
        .num_columns(4)
        .spacing([10.0, 4.0])
        .striped(true)
        .show(ui, |ui| {
            strong_label(ui, "#");
            strong_label(ui, text.stream_type());
            strong_label(ui, text.codec());
            strong_label(ui, text.details());
            ui.end_row();
            for stream in &probe.streams {
                dark_label(ui, stream.index.to_string());
                dark_label(ui, stream.media_type.label());
                dark_label(ui, format!("{} ({})", stream.codec_name, stream.codec_id));
                dark_label(ui, stream_details(stream));
                ui.end_row();
            }
        });
}

pub(crate) fn gop_grid(ui: &mut egui::Ui, gop: &GopInfo, text: Text) {
    section_heading(ui, &format!("GOP {}", gop.index + 1));
    egui::Grid::new("gop_grid")
        .num_columns(2)
        .spacing([12.0, 6.0])
        .striped(true)
        .show(ui, |ui| {
            row(
                ui,
                text.frames(),
                &format!(
                    "{} - {} ({} packets)",
                    gop.start_frame,
                    gop.end_frame,
                    gop.frames()
                ),
            );
            row(
                ui,
                text.time(),
                &format!("{:.3}s + {:.3}s", gop.start_time, gop.duration),
            );
            row(ui, "PTS", &format_timestamp(gop.pts));
            row(ui, "DTS", &format_timestamp(gop.dts));
            row(
                ui,
                text.bytes(),
                &format!(
                    "{} - {} ({})",
                    gop.byte_start,
                    gop.byte_end,
                    format_bytes(gop.bytes())
                ),
            );
        });
}

pub(crate) fn row(ui: &mut egui::Ui, label: &str, value: &str) {
    strong_label(ui, label);
    ui.label(egui::RichText::new(value).color(INK));
    ui.end_row();
}

fn strong_label(ui: &mut egui::Ui, text: &str) {
    ui.label(egui::RichText::new(text).strong().color(INK));
}

fn dark_label(ui: &mut egui::Ui, text: impl Into<String>) {
    ui.label(egui::RichText::new(text.into()).color(INK));
}

pub(crate) fn section_heading(ui: &mut egui::Ui, text: &str) {
    ui.label(egui::RichText::new(text).strong().size(14.0).color(INK));
}

fn format_timestamp(value: i64) -> String {
    if value == i64::MIN {
        "N/A".to_owned()
    } else {
        value.to_string()
    }
}

fn format_rational(num: i32, den: i32) -> String {
    if num <= 0 || den <= 0 {
        "N/A".to_owned()
    } else {
        format!("{num}/{den}")
    }
}

fn format_frame_rate(num: i32, den: i32) -> String {
    if num <= 0 || den <= 0 {
        "N/A".to_owned()
    } else {
        format!("{:.3} ({num}/{den}) FPS", num as f64 / den as f64)
    }
}

pub(crate) fn timeline(ui: &mut egui::Ui, view: TimelineView<'_>) -> Option<TimelineAction> {
    let TimelineView {
        sources,
        tracks,
        active_source,
        current_frame,
        selected,
        edit_selection,
        zoom,
        text,
    } = view;
    ui.horizontal(|ui| {
        ui.heading(text.gop_timeline());
        let total_gops: usize = tracks.iter().map(|track| track.items.len()).sum();
        let max_frames = tracks
            .iter()
            .map(|track| track_edit_frames(sources, track))
            .max()
            .unwrap_or(0);
        ui.label(text.timeline_stats(tracks.len(), total_gops, max_frames, zoom));
    });

    if tracks.is_empty() {
        let desired = egui::vec2(ui.available_width(), 120.0);
        let (rect, _) = ui.allocate_exact_size(desired, egui::Sense::hover());
        ui.painter().rect_filled(rect, 0.0, UI_PANEL);
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            text.timeline_empty(),
            egui::TextStyle::Body.resolve(ui.style()),
            ui.visuals().weak_text_color(),
        );
        return None;
    }

    let pixels_per_frame = zoom.clamp(TIMELINE_MIN_ZOOM, TIMELINE_MAX_ZOOM);
    let max_frames = tracks
        .iter()
        .map(|track| track_edit_frames(sources, track).max(1))
        .max()
        .unwrap_or(1);
    let timeline_width = max_frames as f32 * pixels_per_frame;
    let content_width = (TRACK_LABEL_WIDTH + timeline_width + 14.0).max(ui.available_width());
    let content_height = 30.0 + tracks.len() as f32 * TRACK_HEIGHT + 12.0;
    let mut action = None;
    let viewport_rect = egui::Rect::from_min_size(
        ui.available_rect_before_wrap().min,
        egui::vec2(ui.available_width(), content_height.min(220.0)),
    );
    let zoom_delta = alt_scroll_delta_over(ui, viewport_rect);
    if zoom_delta.abs() > 0.0 {
        let factor = (zoom_delta * 0.004).exp();
        action = Some(TimelineAction::Zoom {
            zoom: (zoom * factor).clamp(TIMELINE_MIN_ZOOM, TIMELINE_MAX_ZOOM),
        });
    }

    egui::ScrollArea::both()
        .auto_shrink([false, false])
        .drag_to_scroll(false)
        .max_height(220.0)
        .show(ui, |ui| {
            let (rect, _response) = ui.allocate_exact_size(
                egui::vec2(content_width, content_height),
                egui::Sense::click(),
            );
            let painter = ui.painter_at(rect);
            painter.rect_filled(rect, 0.0, UI_PANEL);

            let ruler_y = rect.top() + 22.0;
            let track_left = rect.left() + TRACK_LABEL_WIDTH;
            let track_right = track_left + timeline_width;
            let drag_state_id = ui.id().with("timeline-drag-state");
            painter.line_segment(
                [
                    egui::pos2(track_left, ruler_y),
                    egui::pos2(track_right, ruler_y),
                ],
                egui::Stroke::new(1.0, ui.visuals().weak_text_color()),
            );

            let tick_step = choose_tick_step(pixels_per_frame);
            let mut tick = 0;
            let mut last_tick = 0;
            while tick <= max_frames {
                let x = track_left + tick as f32 * pixels_per_frame;
                painter.line_segment(
                    [egui::pos2(x, ruler_y - 5.0), egui::pos2(x, ruler_y + 5.0)],
                    egui::Stroke::new(1.0, ui.visuals().weak_text_color()),
                );
                painter.text(
                    egui::pos2(x + 3.0, rect.top() + 4.0),
                    egui::Align2::LEFT_TOP,
                    tick.to_string(),
                    egui::TextStyle::Small.resolve(ui.style()),
                    ui.visuals().weak_text_color(),
                );
                last_tick = tick;
                tick += tick_step;
            }
            if last_tick != max_frames {
                painter.line_segment(
                    [
                        egui::pos2(track_right, ruler_y - 5.0),
                        egui::pos2(track_right, ruler_y + 5.0),
                    ],
                    egui::Stroke::new(1.0, ui.visuals().weak_text_color()),
                );
            }

            let mut insertion_hint = None;
            let mut floating_drag = None;
            let visible_rect = ui.clip_rect().expand2(egui::vec2(64.0, 0.0));
            let visible_frame_start =
                (((visible_rect.left() - track_left) / pixels_per_frame).floor() as i64 - 4).max(0);
            let visible_frame_end =
                ((visible_rect.right() - track_left) / pixels_per_frame).ceil() as i64 + 4;
            let draw_stride = if pixels_per_frame < 1.0 {
                (1.0 / pixels_per_frame).ceil().max(1.0) as usize
            } else {
                1
            };
            for (track_index, track) in tracks.iter().enumerate() {
                let y = rect.top() + 34.0 + track_index as f32 * TRACK_HEIGHT;
                let row_rect = egui::Rect::from_min_size(
                    egui::pos2(rect.left() + 6.0, y),
                    egui::vec2(content_width - 12.0, TRACK_HEIGHT - 6.0),
                );
                let is_active_track = track
                    .items
                    .iter()
                    .any(|item| active_source == Some(item.source_index));
                let row_bg = if is_active_track {
                    UI_PANEL_ALT
                } else {
                    UI_SURFACE
                };
                painter.rect_filled(row_rect, 0.0, row_bg);
                painter.rect_stroke(
                    row_rect,
                    0.0,
                    egui::Stroke::new(1.0, UI_STROKE),
                    egui::StrokeKind::Inside,
                );
                let swatch_rect = egui::Rect::from_min_size(
                    egui::pos2(rect.left() + 12.0, y + 9.0),
                    egui::vec2(4.0, TRACK_HEIGHT - 22.0),
                );
                painter.rect_filled(swatch_rect, 0.0, track.color);
                painter.text(
                    egui::pos2(rect.left() + 22.0, y + 10.0),
                    egui::Align2::LEFT_TOP,
                    truncate_middle(&track.name, 24),
                    egui::TextStyle::Small.resolve(ui.style()),
                    INK,
                );

                let lane_top = y + 8.0;
                let lane_bottom = y + TRACK_HEIGHT - 12.0;
                let lane_rect = egui::Rect::from_min_max(
                    egui::pos2(track_left, lane_top),
                    egui::pos2(track_right, lane_bottom),
                );
                painter.rect_filled(lane_rect, 0.0, UI_SURFACE);
                painter.rect_stroke(
                    lane_rect,
                    0.0,
                    egui::Stroke::new(1.0, UI_STROKE),
                    egui::StrokeKind::Inside,
                );
                let mut edit_frame = 0;
                for (item_index, item) in track.items.iter().enumerate() {
                    let Some(source) = sources.get(item.source_index) else {
                        continue;
                    };
                    let Some(gop) = source.probe.gops.get(item.gop_index) else {
                        continue;
                    };
                    let frame_count = gop.frames().max(1);
                    let start_frame = edit_frame;
                    edit_frame += frame_count;
                    if start_frame + frame_count < visible_frame_start
                        || start_frame > visible_frame_end
                    {
                        continue;
                    }
                    let draw_item = DrawItem {
                        source,
                        gop,
                        source_index: item.source_index,
                        gop_index: item.gop_index,
                        start_frame,
                        frame_count,
                    };
                    let x0 = track_left + draw_item.start_frame as f32 * pixels_per_frame;
                    let x1 = track_left
                        + (draw_item.start_frame + draw_item.frame_count) as f32 * pixels_per_frame;
                    if x1 < track_left || x0 > track_right {
                        continue;
                    }
                    let block_left = x0.max(track_left);
                    let block_right = x1.min(track_right).max((block_left + 2.0).min(track_right));
                    let block_rect = egui::Rect::from_min_max(
                        egui::pos2(block_left, lane_top),
                        egui::pos2(block_right, lane_bottom),
                    );
                    let is_active_gop = active_source == Some(draw_item.source_index)
                        && selected == Some(draw_item.gop.index);
                    let selection_key = TimelineSelectionKey {
                        track_index,
                        item_index,
                    };
                    let is_edit_selected = edit_selection.contains(&selection_key);
                    if draw_stride > 1
                        && draw_item.frame_count == 1
                        && item_index % draw_stride != 0
                        && !is_active_gop
                        && !is_edit_selected
                    {
                        continue;
                    }
                    let is_active_source = active_source == Some(draw_item.source_index);
                    let fill = if is_edit_selected {
                        pastel_highlight(draw_item.source.color)
                    } else if is_active_source {
                        mix_color(draw_item.source.color, egui::Color32::WHITE, 0.78)
                    } else {
                        mix_color(draw_item.source.color, egui::Color32::WHITE, 0.64)
                    };
                    let stroke_color = if is_active_gop {
                        UI_ACCENT_STRONG
                    } else if is_active_source {
                        UI_ACCENT_SOFT
                    } else {
                        egui::Color32::from_rgb(132, 154, 176)
                    };
                    let drag_key = DragKey {
                        track_index,
                        item_index,
                        source_index: draw_item.source_index,
                        gop_index: draw_item.gop_index,
                    };
                    let block_id = ui.id().with((
                        "gop-block",
                        track_index,
                        item_index,
                        draw_item.source_index,
                        draw_item.gop_index,
                    ));
                    let response = ui.interact(block_rect, block_id, egui::Sense::click_and_drag());
                    if response.drag_started() {
                        let dragged_keys = if is_edit_selected {
                            edit_selection.clone()
                        } else {
                            BTreeSet::from([selection_key])
                        };
                        let previews = drag_preview_items(
                            sources,
                            tracks,
                            edit_selection,
                            drag_key,
                            block_rect,
                            rect,
                            track_left,
                            pixels_per_frame,
                            is_edit_selected,
                            fill,
                            stroke_color,
                        );
                        ui.data_mut(|data| {
                            data.insert_temp(
                                drag_state_id,
                                TimelineDragState {
                                    key: drag_key,
                                    previews,
                                    dragged_keys,
                                },
                            );
                        });
                    }
                    let drag_state =
                        ui.data(|data| data.get_temp::<TimelineDragState>(drag_state_id));
                    let is_dragging = drag_state
                        .as_ref()
                        .is_some_and(|state| state.key == drag_key)
                        && (response.dragged()
                            || response.drag_stopped()
                            || response.drag_started());
                    let is_drag_origin = drag_state
                        .as_ref()
                        .is_some_and(|state| state.dragged_keys.contains(&selection_key));
                    let pointer = response
                        .interact_pointer_pos()
                        .or_else(|| ui.input(|input| input.pointer.interact_pos()));
                    let base_fill = if is_drag_origin {
                        egui::Color32::from_rgba_premultiplied(fill.r(), fill.g(), fill.b(), 38)
                    } else {
                        fill
                    };
                    painter.rect_filled(block_rect, 0.0, base_fill);
                    painter.rect_stroke(
                        block_rect,
                        0.0,
                        if is_drag_origin {
                            egui::Stroke::new(1.0, UI_STROKE)
                        } else {
                            egui::Stroke::new(if is_active_gop { 2.0 } else { 1.0 }, stroke_color)
                        },
                        egui::StrokeKind::Inside,
                    );
                    if is_dragging
                        && let (Some(state), Some(pointer)) = (drag_state.as_ref(), pointer)
                    {
                        floating_drag = Some((pointer, state.previews.clone()));
                    }
                    if !is_drag_origin && block_rect.width() > 22.0 {
                        painter.text(
                            block_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            (draw_item.gop.index + 1).to_string(),
                            egui::TextStyle::Small.resolve(ui.style()),
                            INK,
                        );
                    }
                    if response.clicked() {
                        let modifiers = ui.input(|input| input.modifiers);
                        action = Some(TimelineAction::Seek {
                            track_index,
                            item_index,
                            source_index: draw_item.source_index,
                            frame: draw_item.gop.start_frame,
                            gop_index: draw_item.gop.index,
                            modifiers,
                        });
                    } else if is_dragging && let Some(pointer) = pointer {
                        if let Some((to_track, insert_index)) = insertion_at_pointer(
                            pointer,
                            rect,
                            track_left,
                            pixels_per_frame,
                            tracks,
                            sources,
                        ) {
                            insertion_hint = Some((to_track, insert_index));
                        }
                        if response.drag_stopped()
                            && let Some((to_track, insert_index)) = insertion_at_pointer(
                                pointer,
                                rect,
                                track_left,
                                pixels_per_frame,
                                tracks,
                                sources,
                            )
                            && should_move_dragged_item(
                                track_index,
                                item_index,
                                to_track,
                                insert_index,
                            )
                        {
                            if is_edit_selected {
                                action = Some(TimelineAction::MoveSelectedGops {
                                    to_track,
                                    insert_index,
                                });
                            } else {
                                action = Some(TimelineAction::MoveGop {
                                    from_track: track_index,
                                    to_track,
                                    item_index,
                                    insert_index,
                                });
                            }
                        }
                        if response.drag_stopped() {
                            ui.data_mut(|data| {
                                data.remove_temp::<TimelineDragState>(drag_state_id);
                            });
                        }
                    }
                }
            }

            if let Some((hint_track, insert_index)) = insertion_hint
                && let Some(track) = tracks.get(hint_track)
            {
                let y = rect.top() + 34.0 + hint_track as f32 * TRACK_HEIGHT;
                let lane_top = y + 8.0;
                let lane_bottom = y + TRACK_HEIGHT - 12.0;
                let x = insertion_x(track_left, pixels_per_frame, sources, track, insert_index);
                painter.line_segment(
                    [
                        egui::pos2(x, lane_top - 5.0),
                        egui::pos2(x, lane_bottom + 5.0),
                    ],
                    egui::Stroke::new(2.0, egui::Color32::from_rgb(220, 48, 48)),
                );
            }

            if let Some((pointer, previews)) = floating_drag {
                for preview in previews {
                    let visual_rect =
                        egui::Rect::from_center_size(pointer + preview.offset, preview.size);
                    painter.rect_filled(visual_rect, 0.0, preview.fill);
                    painter.rect_stroke(
                        visual_rect,
                        0.0,
                        egui::Stroke::new(2.0, preview.stroke),
                        egui::StrokeKind::Inside,
                    );
                    if visual_rect.width() > 22.0 {
                        painter.text(
                            visual_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            preview.label.to_string(),
                            egui::TextStyle::Small.resolve(ui.style()),
                            INK,
                        );
                    }
                }
            }

            if let (Some(source_index), Some(frame)) = (active_source, current_frame)
                && let Some((track_index, edit_frame)) =
                    playhead_edit_position(sources, tracks, source_index, frame, selected)
            {
                let x = track_left + edit_frame as f32 * pixels_per_frame + 0.5;
                let y0 = rect.top() + 30.0 + track_index as f32 * TRACK_HEIGHT;
                let y1 = y0 + TRACK_HEIGHT;
                painter.line_segment(
                    [egui::pos2(x, y0), egui::pos2(x, y1)],
                    egui::Stroke::new(2.0, INK),
                );
            }
        });

    action
}

fn alt_scroll_delta_over(ui: &egui::Ui, rect: egui::Rect) -> f32 {
    ui.input_mut(|input| {
        if !input.modifiers.alt {
            return 0.0;
        }
        let Some(pointer) = input.pointer.hover_pos() else {
            return 0.0;
        };
        if !rect.contains(pointer) {
            return 0.0;
        }

        let delta = if input.raw_scroll_delta.y.abs() > 0.0 {
            input.raw_scroll_delta.y
        } else {
            input.smooth_scroll_delta.y
        };
        if delta.abs() > 0.0 {
            input.raw_scroll_delta = egui::Vec2::ZERO;
            input.smooth_scroll_delta = egui::Vec2::ZERO;
        }
        delta
    })
}

#[allow(clippy::too_many_arguments)]
fn drag_preview_items(
    sources: &[SourceEntry],
    tracks: &[TimelineTrack],
    edit_selection: &BTreeSet<TimelineSelectionKey>,
    dragged_key: DragKey,
    dragged_rect: egui::Rect,
    timeline_rect: egui::Rect,
    track_left: f32,
    pixels_per_frame: f32,
    include_selection: bool,
    single_fill: egui::Color32,
    single_stroke: egui::Color32,
) -> Vec<DragPreviewItem> {
    let mut previews = Vec::new();
    if include_selection {
        for key in edit_selection.iter().copied() {
            if let Some(preview) = drag_preview_item_for_key(
                sources,
                tracks,
                key,
                dragged_key,
                dragged_rect,
                timeline_rect,
                track_left,
                pixels_per_frame,
            ) {
                previews.push(preview);
            }
        }
    }

    if previews.is_empty() {
        previews.push(DragPreviewItem {
            offset: egui::Vec2::ZERO,
            size: drag_preview_size(dragged_rect.size()),
            fill: single_fill,
            stroke: single_stroke,
            label: dragged_key.gop_index + 1,
        });
    }
    previews
}

#[allow(clippy::too_many_arguments)]
fn drag_preview_item_for_key(
    sources: &[SourceEntry],
    tracks: &[TimelineTrack],
    key: TimelineSelectionKey,
    dragged_key: DragKey,
    dragged_rect: egui::Rect,
    timeline_rect: egui::Rect,
    track_left: f32,
    pixels_per_frame: f32,
) -> Option<DragPreviewItem> {
    let track = tracks.get(key.track_index)?;
    let item = track.items.get(key.item_index)?;
    let source = sources.get(item.source_index)?;
    let gop = source.probe.gops.get(item.gop_index)?;
    let center = if key.track_index == dragged_key.track_index
        && key.item_index == dragged_key.item_index
        && item.source_index == dragged_key.source_index
        && item.gop_index == dragged_key.gop_index
    {
        dragged_rect.center()
    } else {
        let start_frame = track_edit_frame_before(sources, track, key.item_index);
        let frame_count = gop.frames().max(1);
        let x0 = track_left + start_frame as f32 * pixels_per_frame;
        let x1 = track_left + (start_frame + frame_count) as f32 * pixels_per_frame;
        let y = timeline_rect.top() + 34.0 + key.track_index as f32 * TRACK_HEIGHT;
        egui::pos2((x0 + x1) * 0.5, y + (8.0 + TRACK_HEIGHT - 12.0) * 0.5)
    };
    let width = (gop.frames().max(1) as f32 * pixels_per_frame).max(2.0);
    let height = dragged_rect.height();
    Some(DragPreviewItem {
        offset: center - dragged_rect.center(),
        size: drag_preview_size(egui::vec2(width, height)),
        fill: pastel_highlight(source.color),
        stroke: UI_ACCENT_STRONG,
        label: gop.index + 1,
    })
}

fn drag_preview_size(size: egui::Vec2) -> egui::Vec2 {
    egui::vec2(size.x.clamp(2.0, 260.0), size.y)
}

fn track_edit_frame_before(
    sources: &[SourceEntry],
    track: &TimelineTrack,
    item_index: usize,
) -> i64 {
    track
        .items
        .iter()
        .take(item_index)
        .filter_map(|item| {
            sources
                .get(item.source_index)?
                .probe
                .gops
                .get(item.gop_index)
        })
        .map(|gop| gop.frames().max(1))
        .sum()
}

fn track_edit_frames(sources: &[SourceEntry], track: &TimelineTrack) -> i64 {
    track_edit_frame_before(sources, track, track.items.len())
}

fn insertion_at_pointer(
    pointer: egui::Pos2,
    timeline_rect: egui::Rect,
    track_left: f32,
    pixels_per_frame: f32,
    tracks: &[TimelineTrack],
    sources: &[SourceEntry],
) -> Option<(usize, usize)> {
    if pointer.y < timeline_rect.top() + 34.0 || pointer.x < track_left {
        return None;
    }
    let track_index = ((pointer.y - timeline_rect.top() - 34.0) / TRACK_HEIGHT).floor() as usize;
    let track = tracks.get(track_index)?;
    let pointer_frame = ((pointer.x - track_left) / pixels_per_frame).max(0.0);
    let mut insert_index = track.items.len();
    let mut edit_frame = 0;
    for (index, item) in track.items.iter().enumerate() {
        let Some(source) = sources.get(item.source_index) else {
            continue;
        };
        let Some(gop) = source.probe.gops.get(item.gop_index) else {
            continue;
        };
        let frames = gop.frames().max(1);
        let start = edit_frame;
        let end = edit_frame + frames;
        edit_frame = end;
        let midpoint = (start + end) as f32 * 0.5;
        if pointer_frame < midpoint {
            insert_index = index;
            break;
        }
    }
    Some((track_index, insert_index))
}

fn should_move_dragged_item(
    from_track: usize,
    item_index: usize,
    to_track: usize,
    insert_index: usize,
) -> bool {
    if from_track != to_track {
        return true;
    }
    insert_index != item_index && insert_index != item_index + 1
}

fn insertion_x(
    track_left: f32,
    pixels_per_frame: f32,
    sources: &[SourceEntry],
    track: &TimelineTrack,
    insert_index: usize,
) -> f32 {
    track_left + track_edit_frame_before(sources, track, insert_index) as f32 * pixels_per_frame
}

fn playhead_edit_position(
    sources: &[SourceEntry],
    tracks: &[TimelineTrack],
    source_index: usize,
    frame: i64,
    selected_gop: Option<usize>,
) -> Option<(usize, i64)> {
    for (track_index, track) in tracks.iter().enumerate() {
        let mut edit_frame = 0;
        for item in &track.items {
            let Some(source) = sources.get(item.source_index) else {
                continue;
            };
            let Some(gop) = source.probe.gops.get(item.gop_index) else {
                continue;
            };
            let frames = gop.frames().max(1);
            let selected_matches = selected_gop.is_none_or(|index| index == item.gop_index);
            if item.source_index == source_index
                && selected_matches
                && frame >= gop.start_frame
                && frame <= gop.end_frame
            {
                return Some((track_index, edit_frame + frame - gop.start_frame));
            }
            edit_frame += frames;
        }
    }
    None
}

pub(crate) fn preview_panel(
    ui: &mut egui::Ui,
    texture: Option<&egui::TextureHandle>,
    text: Text,
) -> egui::Rect {
    let available = ui.available_size();
    let desired = egui::vec2(available.x.max(1.0), available.y.max(1.0));
    let (rect, _) = ui.allocate_exact_size(desired, egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, 0.0, ui.visuals().extreme_bg_color);

    if let Some(texture) = texture {
        let image_size = texture.size_vec2();
        let scale = (rect.width() / image_size.x)
            .min(rect.height() / image_size.y)
            .max(0.01);
        let final_size = image_size * scale;
        let image_rect = egui::Rect::from_center_size(rect.center(), final_size);
        ui.painter().image(
            texture.id(),
            image_rect,
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
    } else {
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            text.no_cached_preview(),
            egui::TextStyle::Body.resolve(ui.style()),
            ui.visuals().weak_text_color(),
        );
    }
    rect
}

pub(crate) fn gop_table(
    ui: &mut egui::Ui,
    probe: &ProbeSummary,
    selected: &mut Option<usize>,
    text: Text,
) {
    ui.heading(text.gop_list());
    egui::Grid::new("gop_table_header")
        .num_columns(7)
        .spacing([16.0, 4.0])
        .show(ui, |ui| {
            strong_label(ui, "#");
            strong_label(ui, text.start());
            strong_label(ui, text.end());
            strong_label(ui, text.packets());
            strong_label(ui, text.time());
            strong_label(ui, text.duration());
            strong_label(ui, text.bytes());
            ui.end_row();
        });
    ui.separator();
    let row_height = ui.text_style_height(&egui::TextStyle::Body) + 6.0;
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show_rows(ui, row_height, probe.gops.len(), |ui, row_range| {
            egui::Grid::new("gop_table_rows")
                .striped(true)
                .num_columns(7)
                .spacing([16.0, 4.0])
                .show(ui, |ui| {
                    for row in row_range {
                        let Some(gop) = probe.gops.get(row) else {
                            continue;
                        };
                        let clicked = ui
                            .selectable_label(
                                *selected == Some(gop.index),
                                (gop.index + 1).to_string(),
                            )
                            .clicked();
                        dark_label(ui, gop.start_frame.to_string());
                        dark_label(ui, gop.end_frame.to_string());
                        dark_label(ui, gop.frames().to_string());
                        dark_label(ui, format!("{:.3}", gop.start_time));
                        dark_label(ui, format!("{:.3}", gop.duration));
                        dark_label(ui, format_bytes(gop.bytes()));
                        ui.end_row();
                        if clicked {
                            *selected = Some(gop.index);
                        }
                    }
                });
        });
}
