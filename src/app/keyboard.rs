use eframe::egui;

use crate::app::{GopComposerApp, TimelineEditSnapshot, TimelineItem, TimelineSelectionKey};

const HISTORY_LIMIT: usize = 128;

#[derive(Clone, Copy, Debug)]
struct TimelinePosition {
    track_index: usize,
    item_index: usize,
    item: TimelineItem,
    gop_start: i64,
    gop_end: i64,
}

impl TimelinePosition {
    fn selection_key(self) -> TimelineSelectionKey {
        TimelineSelectionKey {
            track_index: self.track_index,
            item_index: self.item_index,
        }
    }
}

impl GopComposerApp {
    pub(crate) fn handle_keyboard(&mut self, ctx: &egui::Context) {
        if wants_text_input(ctx) {
            return;
        }

        let keys = ctx.input(|input| KeyboardInput {
            left: input.key_pressed(egui::Key::ArrowLeft),
            right: input.key_pressed(egui::Key::ArrowRight),
            up: input.key_pressed(egui::Key::ArrowUp),
            down: input.key_pressed(egui::Key::ArrowDown),
            space: input.key_pressed(egui::Key::Space),
            home: input.key_pressed(egui::Key::Home),
            end: input.key_pressed(egui::Key::End),
            shift: input.modifiers.shift,
            delete: input.key_pressed(egui::Key::Delete),
            copy: input.modifiers.command && input.key_pressed(egui::Key::C),
            cut: input.modifiers.command && input.key_pressed(egui::Key::X),
            paste: input.modifiers.command && input.key_pressed(egui::Key::V),
            undo: input.modifiers.command
                && !input.modifiers.shift
                && input.key_pressed(egui::Key::Z),
            redo: (input.modifiers.command && input.key_pressed(egui::Key::Y))
                || (input.modifiers.command
                    && input.modifiers.shift
                    && input.key_pressed(egui::Key::Z)),
        });

        if keys.undo {
            self.undo_timeline_edit(ctx);
            return;
        }
        if keys.space {
            self.toggle_playback(ctx);
            return;
        }
        if keys.redo {
            self.redo_timeline_edit(ctx);
            return;
        }
        if keys.copy {
            self.copy_selected_gop();
            return;
        }
        if keys.cut {
            self.cut_selected_gop(ctx);
            return;
        }
        if keys.paste {
            self.paste_gop_after_selection(ctx);
            return;
        }
        if keys.delete {
            self.delete_selected_gop(ctx);
            return;
        }

        if self.probe.is_none() || self.current_frame.is_none() {
            return;
        }

        if keys.shift && keys.home {
            self.stop_playback();
            self.select_current_to_file_edge(false);
        } else if keys.shift && keys.end {
            self.stop_playback();
            self.select_current_to_file_edge(true);
        } else if keys.home {
            self.stop_playback();
            self.set_current_frame(0, ctx);
        } else if keys.end {
            self.stop_playback();
            if let Some(max_frame) = self.max_frame() {
                self.set_current_frame(max_frame, ctx);
            }
        } else if keys.up != keys.down {
            self.stop_playback();
            self.seek_gop_delta(if keys.down { 1 } else { -1 }, ctx);
        } else if keys.left != keys.right {
            self.stop_playback();
            self.seek_timeline_delta(if keys.right { 1 } else { -1 }, ctx);
        }
    }

    pub(crate) fn push_timeline_undo(&mut self) {
        self.sync_active_source_state();
        self.undo_stack.push(self.timeline_snapshot());
        if self.undo_stack.len() > HISTORY_LIMIT {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
    }

    fn undo_timeline_edit(&mut self, ctx: &egui::Context) {
        let Some(snapshot) = self.undo_stack.pop() else {
            self.status = "Nothing to undo".to_owned();
            return;
        };
        self.sync_active_source_state();
        let current = self.timeline_snapshot();
        self.redo_stack.push(current);
        self.restore_timeline_snapshot(snapshot, ctx);
        self.status = "Undo timeline edit".to_owned();
    }

    fn redo_timeline_edit(&mut self, ctx: &egui::Context) {
        let Some(snapshot) = self.redo_stack.pop() else {
            self.status = "Nothing to redo".to_owned();
            return;
        };
        self.sync_active_source_state();
        let current = self.timeline_snapshot();
        self.undo_stack.push(current);
        self.restore_timeline_snapshot(snapshot, ctx);
        self.status = "Redo timeline edit".to_owned();
    }

    fn copy_selected_gop(&mut self) {
        let Some(position) = self.current_position() else {
            self.status = "No GOP selected to copy".to_owned();
            return;
        };
        self.timeline_clipboard = Some(position.item);
        self.status = self.item_status("Copied", position.item);
    }

    fn cut_selected_gop(&mut self, ctx: &egui::Context) {
        let Some(position) = self.current_position() else {
            self.status = "No GOP selected to cut".to_owned();
            return;
        };
        self.timeline_clipboard = Some(position.item);
        self.remove_position(position, "Cut", ctx);
    }

    fn delete_selected_gop(&mut self, ctx: &egui::Context) {
        if !self.edit_selection.is_empty() {
            self.delete_edit_selection(ctx);
            return;
        }
        let Some(position) = self.current_position() else {
            self.status = "No GOP selected to delete".to_owned();
            return;
        };
        self.remove_position(position, "Deleted", ctx);
    }

    fn delete_edit_selection(&mut self, ctx: &egui::Context) {
        let selected = self.edit_selection.clone();
        self.push_timeline_undo();
        let mut removed = 0usize;
        let mut fallback = None;
        for (track_index, track) in self.timeline_tracks.iter_mut().enumerate() {
            let mut kept = Vec::with_capacity(track.items.len());
            for (item_index, item) in track.items.drain(..).enumerate() {
                if selected.contains(&TimelineSelectionKey {
                    track_index,
                    item_index,
                }) {
                    removed += 1;
                } else {
                    fallback.get_or_insert(item);
                    kept.push(item);
                }
            }
            track.items = kept;
        }
        self.edit_selection.clear();
        self.selection_anchor = None;
        if let Some(item) = fallback {
            self.seek_item_start(item, ctx);
        }
        self.status = format!(
            "Deleted {} selected GOP{}",
            removed,
            if removed == 1 { "" } else { "s" }
        );
    }

    fn paste_gop_after_selection(&mut self, ctx: &egui::Context) {
        let Some(item) = self.timeline_clipboard else {
            self.status = "Clipboard is empty".to_owned();
            return;
        };
        if self
            .sources
            .get(item.source_index)
            .is_none_or(|source| source.probe.gops.get(item.gop_index).is_none())
        {
            self.status = "Clipboard GOP no longer exists".to_owned();
            return;
        }

        let (track_index, insert_index) = self
            .current_position()
            .map(|position| (position.track_index, position.item_index + 1))
            .or_else(|| self.active_source_track().map(|track| (track, 0)))
            .unwrap_or((0, 0));
        if track_index >= self.timeline_tracks.len() {
            self.status = "No target track for paste".to_owned();
            return;
        }

        self.push_timeline_undo();
        let insert_index = insert_index.min(self.timeline_tracks[track_index].items.len());
        self.timeline_tracks[track_index]
            .items
            .insert(insert_index, item);
        self.edit_selection.clear();
        self.selection_anchor = Some(TimelineSelectionKey {
            track_index,
            item_index: insert_index,
        });
        self.status = self.item_status("Pasted", item);
        self.seek_item_start(item, ctx);
    }

    fn remove_position(&mut self, position: TimelinePosition, verb: &str, ctx: &egui::Context) {
        if self
            .timeline_tracks
            .get(position.track_index)
            .and_then(|track| track.items.get(position.item_index))
            .copied()
            != Some(position.item)
        {
            self.status = "Selected GOP is no longer at that timeline position".to_owned();
            return;
        }

        self.push_timeline_undo();
        self.timeline_tracks[position.track_index]
            .items
            .remove(position.item_index);
        self.edit_selection.clear();
        self.selection_anchor = None;
        self.status = self.item_status(verb, position.item);
        self.seek_after_removal(position, ctx);
    }

    fn seek_gop_delta(&mut self, delta: i64, ctx: &egui::Context) {
        let Some(position) = self.current_position() else {
            return;
        };
        let neighbor_index = if delta > 0 {
            position.item_index.checked_add(1)
        } else {
            position.item_index.checked_sub(1)
        };
        let Some(neighbor_index) = neighbor_index else {
            return;
        };
        let Some(item) = self
            .timeline_tracks
            .get(position.track_index)
            .and_then(|track| track.items.get(neighbor_index))
            .copied()
        else {
            return;
        };
        self.seek_item_start(item, ctx);
    }

    fn seek_timeline_delta(&mut self, delta: i64, ctx: &egui::Context) {
        let (Some(source_index), Some(frame)) = (self.active_source, self.current_frame) else {
            return;
        };
        let Some(position) = self.current_timeline_item(source_index, frame) else {
            self.set_current_frame(frame + delta, ctx);
            return;
        };

        let target = frame + delta;
        if target >= position.gop_start && target <= position.gop_end {
            self.set_current_frame(target, ctx);
            return;
        }

        let neighbor_index = if delta > 0 {
            position.item_index.checked_add(1)
        } else {
            position.item_index.checked_sub(1)
        };
        let Some(neighbor_index) = neighbor_index else {
            self.set_current_frame(target, ctx);
            return;
        };
        let Some(item) = self
            .timeline_tracks
            .get(position.track_index)
            .and_then(|track| track.items.get(neighbor_index))
            .copied()
        else {
            self.set_current_frame(target, ctx);
            return;
        };
        let Some(gop) = self
            .sources
            .get(item.source_index)
            .and_then(|source| source.probe.gops.get(item.gop_index))
        else {
            return;
        };
        let target_frame = if delta > 0 {
            gop.start_frame
        } else {
            gop.end_frame
        };
        self.seek_source_frame(item.source_index, target_frame, ctx);
    }

    fn select_current_to_file_edge(&mut self, to_end: bool) {
        let Some(current) = self.current_position() else {
            self.status = "No GOP selected for range selection".to_owned();
            return;
        };
        let Some(track) = self.timeline_tracks.get(current.track_index) else {
            return;
        };
        let edge = if to_end {
            track.items.len().saturating_sub(1)
        } else {
            0
        };
        self.select_edit_range(
            current.selection_key(),
            TimelineSelectionKey {
                track_index: current.track_index,
                item_index: edge,
            },
        );
    }

    fn current_position(&self) -> Option<TimelinePosition> {
        let (Some(source_index), Some(frame)) = (self.active_source, self.current_frame) else {
            return None;
        };
        self.current_timeline_item(source_index, frame)
    }

    fn current_timeline_item(&self, source_index: usize, frame: i64) -> Option<TimelinePosition> {
        let selected_gop = self.selected_gop;
        let mut fallback = None;
        for (track_index, track) in self.timeline_tracks.iter().enumerate() {
            for (item_index, item) in track.items.iter().enumerate() {
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
                let position = TimelinePosition {
                    track_index,
                    item_index,
                    item: *item,
                    gop_start: gop.start_frame,
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

    fn active_source_track(&self) -> Option<usize> {
        let active_source = self.active_source?;
        self.timeline_tracks.iter().position(|track| {
            track
                .items
                .iter()
                .any(|item| item.source_index == active_source)
        })
    }

    fn seek_after_removal(&mut self, position: TimelinePosition, ctx: &egui::Context) {
        let next_item = self
            .timeline_tracks
            .get(position.track_index)
            .and_then(|track| {
                track.items.get(position.item_index).or_else(|| {
                    position
                        .item_index
                        .checked_sub(1)
                        .and_then(|i| track.items.get(i))
                })
            })
            .copied();
        if let Some(item) = next_item {
            self.seek_item_start(item, ctx);
        } else {
            self.set_current_frame(0, ctx);
        }
    }

    pub(crate) fn seek_item_start(&mut self, item: TimelineItem, ctx: &egui::Context) {
        let Some(frame) = self
            .sources
            .get(item.source_index)
            .and_then(|source| source.probe.gops.get(item.gop_index))
            .map(|gop| gop.start_frame)
        else {
            return;
        };
        self.seek_source_frame(item.source_index, frame, ctx);
    }

    pub(crate) fn seek_source_frame(
        &mut self,
        source_index: usize,
        frame: i64,
        ctx: &egui::Context,
    ) {
        if self.active_source != Some(source_index) {
            self.activate_source(source_index, ctx);
        }
        self.set_current_frame(frame, ctx);
    }

    fn timeline_snapshot(&self) -> TimelineEditSnapshot {
        TimelineEditSnapshot {
            tracks: self.timeline_tracks.clone(),
            edit_selection: self.edit_selection.clone(),
            selection_anchor: self.selection_anchor,
            active_source: self.active_source,
            selected_gop: self.selected_gop,
            current_frame: self.current_frame,
        }
    }

    fn restore_timeline_snapshot(&mut self, snapshot: TimelineEditSnapshot, ctx: &egui::Context) {
        self.timeline_tracks = snapshot.tracks;
        self.edit_selection = snapshot.edit_selection;
        self.selection_anchor = snapshot.selection_anchor;
        if let Some(source_index) = snapshot.active_source
            && source_index < self.sources.len()
        {
            self.activate_source(source_index, ctx);
            if let Some(frame) = snapshot.current_frame {
                self.set_current_frame(frame, ctx);
            } else {
                self.selected_gop = snapshot.selected_gop;
            }
            return;
        }
        self.active_source = None;
        self.clear_active_source_state();
    }

    fn item_status(&self, verb: &str, item: TimelineItem) -> String {
        let source_name = self
            .sources
            .get(item.source_index)
            .map(|source| source.name.as_str())
            .unwrap_or("source");
        format!("{verb} GOP {} from {source_name}", item.gop_index + 1)
    }
}

#[derive(Default)]
struct KeyboardInput {
    left: bool,
    right: bool,
    up: bool,
    down: bool,
    space: bool,
    home: bool,
    end: bool,
    shift: bool,
    delete: bool,
    copy: bool,
    cut: bool,
    paste: bool,
    undo: bool,
    redo: bool,
}

fn wants_text_input(ctx: &egui::Context) -> bool {
    ctx.wants_keyboard_input()
}
