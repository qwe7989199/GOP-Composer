use std::{path::Path, process::Command};

use eframe::egui;

use crate::{
    app::{GopComposerApp, TimelineItem},
    ui::panels::section_heading,
    util::truncate_middle,
};

impl GopComposerApp {
    pub(crate) fn sources_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if self.sources.is_empty() {
            return;
        }

        let text = self.text();
        section_heading(ui, text.sources());
        let mut activate_source = None;
        let mut delete_source = None;
        let mut reveal_source = None;
        let can_delete = self.task.is_none() && self.export_task.is_none();

        for index in 0..self.sources.len() {
            let source = &self.sources[index];
            let label = format!(
                "{}  {} GOPs",
                truncate_middle(&source.name, 24),
                source.probe.gops.len()
            );
            let path = source.path.display().to_string();
            let color = source.color;

            ui.horizontal(|ui| {
                let (swatch, _) =
                    ui.allocate_exact_size(egui::vec2(10.0, 16.0), egui::Sense::hover());
                ui.painter().rect_filled(swatch, 0.0, color);
                let response = ui
                    .selectable_label(self.active_source == Some(index), label)
                    .on_hover_text(path);
                if response.clicked() {
                    activate_source = Some(index);
                }
                let delete_button = egui::Button::new(text.delete());
                if ui.add_enabled(can_delete, delete_button).clicked() {
                    delete_source = Some(index);
                }
                if ui.button(text.show_in_explorer()).clicked() {
                    reveal_source = Some(index);
                }
            });
        }

        if let Some(index) = delete_source {
            self.delete_source(index, ctx);
        } else if let Some(index) = activate_source {
            self.activate_source(index, ctx);
        } else if let Some(index) = reveal_source {
            self.show_source_in_file_manager(index);
        }
        ui.separator();
    }

    fn show_source_in_file_manager(&mut self, source_index: usize) {
        let Some(source) = self.sources.get(source_index) else {
            return;
        };
        match reveal_path(&source.path) {
            Ok(()) => {
                self.status = self.text().opened_source_location(&source.name);
            }
            Err(error) => {
                self.status = self.text().failed_open_source_location(&error);
            }
        }
    }

    pub(crate) fn delete_source(&mut self, source_index: usize, ctx: &egui::Context) {
        if self.task.is_some() {
            self.status = self.text().wait_indexing_delete().to_owned();
            return;
        }
        if self.export_task.is_some() {
            self.status = self.text().wait_export_delete().to_owned();
            return;
        }
        if source_index >= self.sources.len() {
            return;
        }

        self.stop_playback();
        self.sync_active_source_state();
        let old_active = self.active_source;
        let removed = self.sources.remove(source_index);
        self.probe_progress.remove(&removed.path);

        let mut kept_tracks = Vec::with_capacity(self.timeline_tracks.len());
        for mut track in self.timeline_tracks.drain(..) {
            let mut kept = Vec::with_capacity(track.items.len());
            for mut item in track.items.drain(..) {
                if remap_source_index(&mut item, source_index) {
                    kept.push(item);
                }
            }
            track.items = kept;
            if !track.items.is_empty() {
                kept_tracks.push(track);
            }
        }
        self.timeline_tracks = kept_tracks;

        self.edit_selection.clear();
        self.selection_anchor = None;
        self.timeline_clipboard = None;
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.active_source = None;
        self.clear_active_source_state();

        if self.sources.is_empty() {
            self.status = self.text().deleted_source(&removed.name);
            return;
        }

        let next_active = match old_active {
            Some(active) if active == source_index => source_index.min(self.sources.len() - 1),
            Some(active) if active > source_index => active - 1,
            Some(active) if active < self.sources.len() => active,
            _ => 0,
        };
        self.activate_source(next_active, ctx);
        self.status = self.text().deleted_source_tracks(&removed.name);
    }
}

fn remap_source_index(item: &mut TimelineItem, removed_source: usize) -> bool {
    if item.source_index == removed_source {
        return false;
    }
    if item.source_index > removed_source {
        item.source_index -= 1;
    }
    true
}

#[cfg(target_os = "windows")]
fn reveal_path(path: &Path) -> Result<(), String> {
    Command::new("explorer.exe")
        .arg(format!("/select,{}", path.display()))
        .spawn()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[cfg(target_os = "macos")]
fn reveal_path(path: &Path) -> Result<(), String> {
    Command::new("open")
        .arg("-R")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
fn reveal_path(path: &Path) -> Result<(), String> {
    let target = path.parent().unwrap_or(path);
    Command::new("xdg-open")
        .arg(target)
        .spawn()
        .map(|_| ())
        .map_err(|error| error.to_string())
}
