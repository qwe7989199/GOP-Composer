use eframe::egui;

const TITLE: &str = "GOP Composer";
const TITLE_HEIGHT: f32 = 34.0;
const BUTTON_WIDTH: f32 = 46.0;
const RESIZE_MARGIN: f32 = 5.0;
const TITLE_BG: egui::Color32 = egui::Color32::from_rgb(226, 241, 255);
const TEXT: egui::Color32 = egui::Color32::from_rgb(42, 53, 68);
const SUB: egui::Color32 = egui::Color32::from_rgb(91, 111, 130);
const BORDER: egui::Color32 = egui::Color32::from_rgb(137, 174, 207);
const SEPARATOR: egui::Color32 = egui::Color32::from_rgb(189, 211, 232);

pub(crate) fn handle_resize(ctx: &egui::Context) {
    use egui::viewport::ResizeDirection;

    let Some(pos) = ctx.input(|input| input.pointer.hover_pos()) else {
        return;
    };
    let rect = ctx.screen_rect();

    let left = pos.x < rect.left() + RESIZE_MARGIN;
    let right = pos.x > rect.right() - RESIZE_MARGIN;
    let top = pos.y < rect.top() + RESIZE_MARGIN;
    let bottom = pos.y > rect.bottom() - RESIZE_MARGIN;

    let direction = match (left, right, top, bottom) {
        (true, false, true, false) => Some(ResizeDirection::NorthWest),
        (false, true, true, false) => Some(ResizeDirection::NorthEast),
        (true, false, false, true) => Some(ResizeDirection::SouthWest),
        (false, true, false, true) => Some(ResizeDirection::SouthEast),
        (true, false, false, false) => Some(ResizeDirection::West),
        (false, true, false, false) => Some(ResizeDirection::East),
        (false, false, true, false) => Some(ResizeDirection::North),
        (false, false, false, true) => Some(ResizeDirection::South),
        _ => None,
    };

    if let Some(direction) = direction {
        ctx.set_cursor_icon(resize_cursor(direction));
        if ctx.input(|input| input.pointer.primary_pressed()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(direction));
        }
    }
}

pub(crate) fn draw_title_bar(ctx: &egui::Context) -> bool {
    let mut close_clicked = false;
    egui::TopBottomPanel::top("title_bar")
        .exact_height(TITLE_HEIGHT)
        .frame(
            egui::Frame::new()
                .fill(TITLE_BG)
                .inner_margin(egui::Margin::ZERO),
        )
        .show(ctx, |ui| {
            let rect = ui.max_rect();
            let painter = ui.painter();

            let close_rect = title_button_rect(rect, 1);
            let max_rect = title_button_rect(rect, 2);
            let min_rect = title_button_rect(rect, 3);
            let drag_rect =
                egui::Rect::from_min_max(rect.min, egui::pos2(min_rect.left(), rect.bottom()));

            let drag_response = ui.interact(
                drag_rect,
                ui.id().with("title_drag"),
                egui::Sense::click_and_drag(),
            );
            if drag_response.drag_started() {
                ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }
            if drag_response.double_clicked() {
                toggle_maximized(ctx);
            }

            draw_title(painter, rect);
            painter.line_segment(
                [
                    egui::pos2(rect.left(), rect.bottom() - 0.5),
                    egui::pos2(rect.right(), rect.bottom() - 0.5),
                ],
                egui::Stroke::new(1.0, SEPARATOR),
            );
            draw_minimize_button(ui, painter, min_rect, ctx);
            draw_maximize_button(ui, painter, max_rect, ctx);
            close_clicked = draw_close_button(ui, painter, close_rect);
        });
    close_clicked
}

pub(crate) fn draw_window_border(ctx: &egui::Context) {
    let rect = ctx.screen_rect().shrink(0.75);
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("window-border"),
    ));
    painter.rect_stroke(
        rect,
        0.0,
        egui::Stroke::new(1.0, BORDER),
        egui::StrokeKind::Inside,
    );
}

fn draw_title(painter: &egui::Painter, rect: egui::Rect) {
    painter.text(
        egui::pos2(rect.left() + 14.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        TITLE,
        egui::FontId::proportional(14.0),
        TEXT,
    );
}

fn draw_minimize_button(
    ui: &egui::Ui,
    painter: &egui::Painter,
    rect: egui::Rect,
    ctx: &egui::Context,
) {
    let response = ui.interact(rect, ui.id().with("btn_min"), egui::Sense::click());
    if response.hovered() {
        painter.rect_filled(
            rect,
            0.0,
            egui::Color32::from_rgba_unmultiplied(255, 255, 255, 90),
        );
    }
    let center = rect.center();
    painter.line_segment(
        [
            egui::pos2(center.x - 5.0, center.y),
            egui::pos2(center.x + 5.0, center.y),
        ],
        egui::Stroke::new(1.5, SUB),
    );
    if response.clicked() {
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
    }
}

fn draw_maximize_button(
    ui: &egui::Ui,
    painter: &egui::Painter,
    rect: egui::Rect,
    ctx: &egui::Context,
) {
    let response = ui.interact(rect, ui.id().with("btn_max"), egui::Sense::click());
    if response.hovered() {
        painter.rect_filled(
            rect,
            0.0,
            egui::Color32::from_rgba_unmultiplied(255, 255, 255, 90),
        );
    }

    let center = rect.center();
    let size = 5.0;
    let maximized = ctx.input(|input| input.viewport().maximized.unwrap_or(false));
    if maximized {
        draw_restore_icon(painter, center, size);
    } else {
        painter.rect_stroke(
            egui::Rect::from_center_size(center, egui::vec2(size * 2.0, size * 2.0)),
            0.0,
            egui::Stroke::new(1.2, SUB),
            egui::StrokeKind::Outside,
        );
    }
    if response.clicked() {
        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
    }
}

fn draw_close_button(ui: &egui::Ui, painter: &egui::Painter, rect: egui::Rect) -> bool {
    let response = ui.interact(rect, ui.id().with("btn_close"), egui::Sense::click());
    let (bg, fg) = if response.hovered() {
        (
            egui::Color32::from_rgb(202, 226, 248),
            egui::Color32::from_rgb(42, 53, 68),
        )
    } else {
        (egui::Color32::TRANSPARENT, SUB)
    };
    painter.rect_filled(rect, 0.0, bg);
    let center = rect.center();
    let size = 5.0;
    painter.line_segment(
        [
            egui::pos2(center.x - size, center.y - size),
            egui::pos2(center.x + size, center.y + size),
        ],
        egui::Stroke::new(1.5, fg),
    );
    painter.line_segment(
        [
            egui::pos2(center.x + size, center.y - size),
            egui::pos2(center.x - size, center.y + size),
        ],
        egui::Stroke::new(1.5, fg),
    );
    response.clicked()
}

fn draw_restore_icon(painter: &egui::Painter, center: egui::Pos2, size: f32) {
    let offset = 2.0;
    let back = egui::Rect::from_min_size(
        egui::pos2(center.x - size + offset, center.y - size - offset),
        egui::vec2(size * 2.0 - offset, size * 2.0 - offset),
    );
    painter.rect_stroke(
        back,
        0.0,
        egui::Stroke::new(1.2, SUB),
        egui::StrokeKind::Outside,
    );
    let front = egui::Rect::from_min_size(
        egui::pos2(center.x - size, center.y - size + offset),
        egui::vec2(size * 2.0 - offset, size * 2.0 - offset),
    );
    painter.rect_filled(front, 0.0, TITLE_BG);
    painter.rect_stroke(
        front,
        0.0,
        egui::Stroke::new(1.2, SUB),
        egui::StrokeKind::Outside,
    );
}

fn title_button_rect(window_rect: egui::Rect, from_right: usize) -> egui::Rect {
    egui::Rect::from_min_size(
        egui::pos2(
            window_rect.right() - BUTTON_WIDTH * from_right as f32,
            window_rect.top(),
        ),
        egui::vec2(BUTTON_WIDTH, TITLE_HEIGHT),
    )
}

fn toggle_maximized(ctx: &egui::Context) {
    let maximized = ctx.input(|input| input.viewport().maximized.unwrap_or(false));
    ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
}

fn resize_cursor(direction: egui::viewport::ResizeDirection) -> egui::CursorIcon {
    use egui::viewport::ResizeDirection;
    match direction {
        ResizeDirection::North => egui::CursorIcon::ResizeNorth,
        ResizeDirection::South => egui::CursorIcon::ResizeSouth,
        ResizeDirection::East => egui::CursorIcon::ResizeEast,
        ResizeDirection::West => egui::CursorIcon::ResizeWest,
        ResizeDirection::NorthEast => egui::CursorIcon::ResizeNorthEast,
        ResizeDirection::NorthWest => egui::CursorIcon::ResizeNorthWest,
        ResizeDirection::SouthEast => egui::CursorIcon::ResizeSouthEast,
        ResizeDirection::SouthWest => egui::CursorIcon::ResizeSouthWest,
    }
}
