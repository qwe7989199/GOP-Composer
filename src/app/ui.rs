use eframe::egui;

use crate::{
    app::{GopComposerApp, TimelineAction, TimelineSelectionKey},
    media::ffmpeg::ExportKind,
    ui::{
        panel_lines::{draw_bottom_separator, draw_left_separator, draw_top_separator},
        panels::{
            TimelineView, empty_state, gop_grid, gop_table, row, streams_grid, summary_grid,
            timeline,
        },
        window_chrome,
    },
};

const TIMELINE_MIN_ZOOM: f32 = 0.05;
const TIMELINE_MAX_ZOOM: f32 = 24.0;
const UI_INK: egui::Color32 = egui::Color32::from_rgb(42, 53, 68);
const UI_PANEL: egui::Color32 = egui::Color32::from_rgb(236, 246, 255);
const UI_SURFACE: egui::Color32 = egui::Color32::from_rgb(248, 252, 255);

impl GopComposerApp {
    pub(crate) fn update_app_ui(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        let text = self.text();
        if ctx.input(|input| input.viewport().close_requested()) {
            self.begin_close();
        }
        if self.close_requested {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        self.poll_probe_task(ctx);
        self.poll_preview_task(ctx);
        self.poll_export_task(ctx);
        self.handle_keyboard(ctx);
        self.tick_playback(ctx);
        window_chrome::handle_resize(ctx);
        if window_chrome::draw_title_bar(ctx) {
            self.request_close(ctx);
            return;
        }
        if self.close_requested {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        let mut export_action = None;
        let top_bar_response = egui::TopBottomPanel::top("top_bar")
            .frame(
                egui::Frame::NONE
                    .fill(UI_PANEL)
                    .inner_margin(egui::Margin::symmetric(10, 6)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.button(text.open_main_video()).clicked() {
                        self.open_videos(true);
                    }
                    if ui.button(text.add_videos()).clicked() {
                        self.open_videos(false);
                    }
                    if ui.button(text.clear()).clicked() {
                        self.clear();
                    }
                    ui.separator();
                    let play_label = if self.playback_active {
                        text.pause()
                    } else {
                        text.play()
                    };
                    if ui.button(play_label).clicked() {
                        self.toggle_playback(ctx);
                    }
                    if ui.button(text.gop_list()).clicked() {
                        self.show_gop_table = true;
                    }
                    ui.separator();
                    let export_enabled = self.export_task.is_none()
                        && self
                            .timeline_tracks
                            .first()
                            .is_some_and(|track| !track.items.is_empty());
                    if export_enabled {
                        self.mp4_export_options_ui(ui);
                    }
                    if ui
                        .add_enabled(export_enabled, egui::Button::new(text.export_mp4()))
                        .clicked()
                    {
                        export_action = Some(ExportKind::Mp4);
                    }
                    if ui
                        .add_enabled(export_enabled, egui::Button::new(text.export_raw()))
                        .clicked()
                    {
                        export_action = Some(ExportKind::Raw);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if globe_language_button(ui, text.switch_language()).clicked() {
                            self.toggle_language();
                        }
                        if ui.button(text.about()).clicked() {
                            self.show_about = true;
                        }
                    });
                });
            });
        draw_bottom_separator(&top_bar_response.response);

        let inspector_response = egui::SidePanel::right("inspector")
            .frame(
                egui::Frame::NONE
                    .fill(UI_SURFACE)
                    .inner_margin(egui::Margin::symmetric(10, 8)),
            )
            .resizable(true)
            .default_width(420.0)
            .max_width(520.0)
            .show(ctx, |ui| {
                ui.visuals_mut().override_text_color = Some(UI_INK);
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new(text.inspector())
                                .strong()
                                .size(18.0)
                                .color(UI_INK),
                        );
                        ui.separator();
                        self.sources_ui(ui, ctx);
                        if let Some(probe) = &self.probe {
                            summary_grid(ui, probe, text);
                            self.compatibility_ui(ui);
                            if !probe.streams.is_empty() {
                                ui.separator();
                                streams_grid(ui, probe, text);
                            }
                            ui.separator();
                            if let Some(frame) = self.current_frame {
                                row(ui, text.current_frame(), &frame.to_string());
                            }
                            if let Some(gop) =
                                self.selected_gop.and_then(|index| probe.gops.get(index))
                            {
                                ui.separator();
                                gop_grid(ui, gop, text);
                            } else {
                                ui.label(text.select_gop());
                            }
                        } else {
                            ui.label(text.open_video_hint());
                        }
                    });
            });
        draw_left_separator(&inspector_response.response);

        self.sync_active_source_state();

        let mut timeline_action = None;
        let mut selected_gop = self.selected_gop;
        let status_response = egui::TopBottomPanel::bottom("status_bar")
            .frame(
                egui::Frame::NONE
                    .fill(UI_PANEL)
                    .inner_margin(egui::Margin::symmetric(10, 5)),
            )
            .show(ctx, |ui| {
                ui.visuals_mut().override_text_color = Some(UI_INK);
                ui.horizontal_wrapped(|ui| {
                    ui.strong(text.status());
                    ui.separator();
                    ui.label(&self.status);
                    if let (Some(probe), Some(frame)) = (&self.probe, self.current_frame) {
                        ui.separator();
                        ui.label(format!(
                            "{} {} / {}",
                            text.frame(),
                            frame,
                            probe.frame_count.saturating_sub(1)
                        ));
                    }
                    if !self.preview_status.is_empty() {
                        ui.separator();
                        ui.label(&self.preview_status);
                    }
                    if !self.export_status.is_empty() {
                        ui.separator();
                        ui.label(&self.export_status);
                    }
                });
                if self.task.is_some()
                    || self
                        .probe_progress
                        .values()
                        .any(|item| item.error.is_some())
                {
                    self.probe_progress_ui(ui);
                }
            });
        draw_top_separator(&status_response.response);

        let timeline_response = egui::TopBottomPanel::bottom("timeline_panel")
            .frame(
                egui::Frame::NONE
                    .fill(UI_PANEL)
                    .inner_margin(egui::Margin::symmetric(8, 6)),
            )
            .resizable(false)
            .show(ctx, |ui| {
                ui.visuals_mut().override_text_color = Some(UI_INK);
                if self.probe.is_some() {
                    timeline_action = timeline(
                        ui,
                        TimelineView {
                            sources: &self.sources,
                            tracks: &self.timeline_tracks,
                            active_source: self.active_source,
                            current_frame: self.current_frame,
                            selected: selected_gop,
                            edit_selection: &self.edit_selection,
                            zoom: self.timeline_zoom,
                            text,
                        },
                    );
                    ui.add(
                        egui::Slider::new(
                            &mut self.timeline_zoom,
                            TIMELINE_MIN_ZOOM..=TIMELINE_MAX_ZOOM,
                        )
                        .logarithmic(true)
                        .text("px/frame"),
                    );
                }
            });
        draw_top_separator(&timeline_response.response);

        let mut preview_rect = None;
        egui::CentralPanel::default()
            .frame(
                egui::Frame::NONE
                    .fill(UI_SURFACE)
                    .inner_margin(egui::Margin::symmetric(10, 8)),
            )
            .show(ctx, |ui| {
                ui.visuals_mut().override_text_color = Some(UI_INK);
                if self.probe.is_some() {
                    ui.horizontal(|ui| {
                        ui.heading(text.preview());
                        if let Some(frame) = self.current_frame {
                            ui.label(format!("{} {frame}", text.frame()));
                        }
                    });
                    ui.add_space(4.0);
                    preview_rect = self.draw_preview_panel(ui, ctx);
                } else {
                    empty_state(ui, text);
                }
            });

        let mut gop_list_rect = None;
        if self.show_gop_table
            && let Some(probe) = self.probe.clone()
            && let Some(response) = egui::Window::new(text.gop_list())
                .open(&mut self.show_gop_table)
                .frame(
                    egui::Frame::window(&ctx.style())
                        .fill(UI_SURFACE)
                        .stroke(egui::Stroke::new(
                            1.0,
                            egui::Color32::from_rgb(189, 211, 232),
                        ))
                        .corner_radius(egui::CornerRadius::ZERO),
                )
                .default_width(760.0)
                .default_height(520.0)
                .resizable(true)
                .show(ctx, |ui| {
                    gop_table(ui, &probe, &mut selected_gop, text);
                })
        {
            gop_list_rect = Some(response.response.rect);
        }
        self.about_window_ui(ctx, text);

        if let Some(action) = timeline_action {
            match action {
                TimelineAction::Seek {
                    track_index,
                    item_index,
                    source_index,
                    frame,
                    gop_index,
                    modifiers,
                } => {
                    self.handle_timeline_gop_click(
                        TimelineSelectionKey {
                            track_index,
                            item_index,
                        },
                        source_index,
                        gop_index,
                        frame,
                        modifiers,
                        ctx,
                    );
                }
                TimelineAction::Zoom { zoom } => {
                    self.timeline_zoom = zoom.clamp(TIMELINE_MIN_ZOOM, TIMELINE_MAX_ZOOM);
                }
                TimelineAction::MoveGop {
                    from_track,
                    to_track,
                    item_index,
                    insert_index,
                } => {
                    self.move_gop_between_tracks(
                        from_track,
                        to_track,
                        item_index,
                        insert_index,
                        ctx,
                    );
                }
                TimelineAction::MoveSelectedGops {
                    to_track,
                    insert_index,
                } => {
                    self.move_selected_gops(to_track, insert_index, ctx);
                }
            }
        } else if selected_gop != self.selected_gop {
            self.select_gop(selected_gop, ctx);
        }

        let _ = (frame, preview_rect, gop_list_rect);

        if let Some(kind) = export_action {
            self.start_selected_gop_export(kind, ctx);
        }
        self.timecode_apply_prompt_ui(ctx);
        window_chrome::draw_window_border(ctx);
    }

    fn mp4_export_options_ui(&mut self, ui: &mut egui::Ui) {
        let text = self.text();
        ui.checkbox(&mut self.export_include_audio, text.mux_audio());
        let Some(((num, den), source_time_base_ok)) = self.main_track_time_base_status() else {
            return;
        };
        if self.export_time_base_num <= 0 || self.export_time_base_den <= 0 {
            self.export_time_base_num = num;
            self.export_time_base_den = den;
        }
        if !source_time_base_ok {
            self.export_use_source_time_base = false;
        }
        let response = ui.add_enabled(
            source_time_base_ok,
            egui::Checkbox::new(
                &mut self.export_use_source_time_base,
                text.source_timebase(),
            ),
        );
        if !source_time_base_ok {
            response.on_hover_text(text.source_timebase_tip());
        }
        if !self.export_use_source_time_base {
            ui.add(
                egui::DragValue::new(&mut self.export_time_base_num)
                    .range(1..=i32::MAX)
                    .speed(1)
                    .prefix("TB "),
            );
            ui.label("/");
            ui.add(
                egui::DragValue::new(&mut self.export_time_base_den)
                    .range(1..=i32::MAX)
                    .speed(100),
            );
        }
        ui.checkbox(&mut self.export_generate_pts, text.generate_pts());
        if self.export_generate_pts {
            ui.add(
                egui::DragValue::new(&mut self.export_pts_step)
                    .range(0..=i64::MAX)
                    .speed(1)
                    .prefix("PTS Step "),
            )
            .on_hover_text(text.pts_step_tip());
        }
    }

    fn about_window_ui(&mut self, ctx: &egui::Context, text: crate::ui::i18n::Text) {
        if !self.show_about {
            return;
        }

        egui::Window::new(text.about_title())
            .open(&mut self.show_about)
            .collapsible(false)
            .resizable(false)
            .default_width(520.0)
            .frame(
                egui::Frame::window(&ctx.style())
                    .fill(UI_SURFACE)
                    .stroke(egui::Stroke::new(
                        1.0,
                        egui::Color32::from_rgb(117, 159, 196),
                    ))
                    .corner_radius(egui::CornerRadius::ZERO),
            )
            .show(ctx, |ui| {
                ui.visuals_mut().override_text_color = Some(UI_INK);
                ui.label(
                    egui::RichText::new("GOP Composer")
                        .strong()
                        .size(20.0)
                        .color(UI_INK),
                );
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.strong(text.author());
                    ui.label("domo");
                });
                ui.horizontal(|ui| {
                    ui.strong(text.contact());
                    ui.label("domo@vmoe.info");
                });
                ui.separator();
                ui.label(
                    egui::RichText::new(text.project_intro_title())
                        .strong()
                        .size(14.0)
                        .color(UI_INK),
                );
                ui.label(text.project_intro());
                ui.add_space(6.0);
                ui.label(text.local_processing_note());
            });
    }
}

fn globe_language_button(ui: &mut egui::Ui, tooltip: &str) -> egui::Response {
    let desired = egui::vec2(28.0, 24.0);
    let (rect, response) = ui.allocate_exact_size(desired, egui::Sense::click());
    let response = response.on_hover_text(tooltip);
    let visuals = ui.style().interact(&response);
    let painter = ui.painter();

    painter.rect_filled(rect, 0.0, visuals.bg_fill);
    painter.rect_stroke(rect, 0.0, visuals.bg_stroke, egui::StrokeKind::Inside);

    // Geometry follows Lucide `globe.svg` (lucide-static, ISC):
    // circle cx=12 cy=12 r=10; path M12 2a14.5 14.5 0 0 0 0 20 ...; path M2 12h20.
    let icon_rect = egui::Rect::from_center_size(rect.center(), egui::vec2(17.5, 17.5));
    let stroke = egui::Stroke::new(1.55, visuals.fg_stroke.color);
    let soft_fill = egui::Color32::from_rgba_premultiplied(
        visuals.fg_stroke.color.r(),
        visuals.fg_stroke.color.g(),
        visuals.fg_stroke.color.b(),
        14,
    );
    let center = svg_point(icon_rect, 12.0, 12.0);
    let radius = icon_rect.width() * 10.0 / 24.0;
    painter.circle_filled(center, radius, soft_fill);
    painter.circle_stroke(center, radius, stroke);
    painter.line_segment(
        [
            svg_point(icon_rect, 2.0, 12.0),
            svg_point(icon_rect, 22.0, 12.0),
        ],
        stroke,
    );
    painter.add(egui::Shape::line(
        lucide_globe_meridian(icon_rect, false),
        stroke,
    ));
    painter.add(egui::Shape::line(
        lucide_globe_meridian(icon_rect, true),
        stroke,
    ));

    response
}

fn svg_point(rect: egui::Rect, x: f32, y: f32) -> egui::Pos2 {
    egui::pos2(
        rect.left() + rect.width() * x / 24.0,
        rect.top() + rect.height() * y / 24.0,
    )
}

fn lucide_globe_meridian(rect: egui::Rect, mirror: bool) -> Vec<egui::Pos2> {
    let center = svg_point(rect, 12.0, 12.0);
    let rx = rect.width() * 4.1 / 24.0;
    let ry = rect.height() * 10.0 / 24.0;
    (0..=28)
        .map(|index| {
            let t = -std::f32::consts::FRAC_PI_2 + std::f32::consts::PI * index as f32 / 28.0;
            let x = if mirror { -rx * t.cos() } else { rx * t.cos() };
            egui::pos2(center.x + x, center.y + ry * t.sin())
        })
        .collect()
}
