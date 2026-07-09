use eframe::egui;

use crate::app::GopComposerApp;

impl GopComposerApp {
    pub(crate) fn begin_close(&mut self) {
        if self.close_requested {
            return;
        }
        self.close_requested = true;
        self.stop_playback();
        self.preview_request = None;
        self.preview_worker = None;
        self.preview_pending_frame = None;
        self.preview_pending_decoded.clear();
        self.preview_status.clear();
        self.gl_preview.suspend();
        self.status = self.text().closing().to_owned();
    }

    pub(crate) fn request_close(&mut self, ctx: &egui::Context) {
        self.begin_close();
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    pub(crate) fn finish_close(&mut self) {
        self.stop_playback();
        self.preview_request = None;
        self.preview_worker = None;
        self.preview_pending_frame = None;
        self.preview_pending_decoded.clear();
        self.preview_cache.clear();
        self.preview_display_frame = None;
        self.preview_status.clear();
        self.gl_preview.reset();
    }
}
