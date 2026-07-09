use eframe::egui;

const PANEL_LINE: egui::Color32 = egui::Color32::from_rgb(189, 211, 232);

pub(crate) fn draw_top_separator(response: &egui::Response) {
    let rect = response.rect;
    draw_separator(
        response,
        egui::pos2(rect.left(), rect.top() + 0.5),
        egui::pos2(rect.right(), rect.top() + 0.5),
    );
}

pub(crate) fn draw_bottom_separator(response: &egui::Response) {
    let rect = response.rect;
    draw_separator(
        response,
        egui::pos2(rect.left(), rect.bottom() - 0.5),
        egui::pos2(rect.right(), rect.bottom() - 0.5),
    );
}

pub(crate) fn draw_left_separator(response: &egui::Response) {
    let rect = response.rect;
    draw_separator(
        response,
        egui::pos2(rect.left() + 0.5, rect.top()),
        egui::pos2(rect.left() + 0.5, rect.bottom()),
    );
}

fn draw_separator(response: &egui::Response, start: egui::Pos2, end: egui::Pos2) {
    let painter = response.ctx.layer_painter(response.layer_id);
    painter.line_segment([start, end], egui::Stroke::new(1.0, PANEL_LINE));
}
