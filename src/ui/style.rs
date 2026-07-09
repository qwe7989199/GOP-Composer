use eframe::egui;

pub(crate) fn install_hard_style(ctx: &egui::Context) {
    ctx.all_styles_mut(apply_hard_style);
}

fn apply_hard_style(style: &mut egui::Style) {
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(9.0, 4.0);
    style.spacing.window_margin = egui::Margin::same(8);
    style.spacing.menu_margin = egui::Margin::same(6);
    style.spacing.interact_size.y = 24.0;

    let zero = egui::CornerRadius::ZERO;
    let ink = egui::Color32::from_rgb(42, 53, 68);
    let accent = egui::Color32::from_rgb(92, 142, 192);
    style.visuals = egui::Visuals::light();
    style.visuals.window_corner_radius = zero;
    style.visuals.menu_corner_radius = zero;
    style.visuals.window_fill = egui::Color32::from_rgb(248, 252, 255);
    style.visuals.panel_fill = egui::Color32::from_rgb(236, 246, 255);
    style.visuals.extreme_bg_color = egui::Color32::from_rgb(222, 238, 252);
    style.visuals.faint_bg_color = egui::Color32::from_rgb(232, 244, 255);
    style.visuals.window_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(189, 211, 232));
    style.visuals.hyperlink_color = accent;
    style.visuals.warn_fg_color = egui::Color32::from_rgb(170, 124, 61);
    style.visuals.error_fg_color = egui::Color32::from_rgb(176, 82, 98);
    style.visuals.selection.bg_fill = egui::Color32::from_rgb(158, 207, 244);
    style.visuals.selection.stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(58, 103, 154));

    style.visuals.widgets.noninteractive.corner_radius = zero;
    style.visuals.widgets.inactive.corner_radius = zero;
    style.visuals.widgets.hovered.corner_radius = zero;
    style.visuals.widgets.active.corner_radius = zero;
    style.visuals.widgets.open.corner_radius = zero;

    style.visuals.widgets.noninteractive.weak_bg_fill = egui::Color32::from_rgb(248, 252, 255);
    style.visuals.widgets.noninteractive.bg_fill = egui::Color32::from_rgb(248, 252, 255);
    style.visuals.widgets.noninteractive.bg_stroke =
        egui::Stroke::new(1.0, egui::Color32::from_rgb(196, 216, 235));
    style.visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, ink);

    style.visuals.widgets.inactive.weak_bg_fill = egui::Color32::from_rgb(248, 252, 255);
    style.visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(248, 252, 255);
    style.visuals.widgets.inactive.bg_stroke =
        egui::Stroke::new(1.0, egui::Color32::from_rgb(182, 206, 228));
    style.visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, ink);

    style.visuals.widgets.hovered.weak_bg_fill = egui::Color32::from_rgb(225, 241, 255);
    style.visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(225, 241, 255);
    style.visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, accent);
    style.visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, ink);

    style.visuals.widgets.active.weak_bg_fill = egui::Color32::from_rgb(184, 220, 248);
    style.visuals.widgets.active.bg_fill = egui::Color32::from_rgb(184, 220, 248);
    style.visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, accent);
    style.visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, ink);

    style.visuals.widgets.open.weak_bg_fill = egui::Color32::from_rgb(214, 234, 252);
    style.visuals.widgets.open.bg_fill = egui::Color32::from_rgb(214, 234, 252);
    style.visuals.widgets.open.bg_stroke = egui::Stroke::new(1.0, accent);
    style.visuals.widgets.open.fg_stroke = egui::Stroke::new(1.0, ink);
}
