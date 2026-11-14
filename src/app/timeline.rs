use eframe::egui;

use super::{GopComposerApp, TimelineSelectionKey};

impl GopComposerApp {
    pub(crate) fn move_gop_between_tracks(
        &mut self,
        from_track: usize,
        to_track: usize,
        item_index: usize,
        insert_index: usize,
        ctx: &egui::Context,
    ) {
        if from_track >= self.timeline_tracks.len() || to_track >= self.timeline_tracks.len() {
            return;
        }
        self.sync_active_source_state();

        if item_index >= self.timeline_tracks[from_track].items.len() {
            return;
        }
        self.push_timeline_undo();
        let moved = self.timeline_tracks[from_track].items.remove(item_index);
        let mut insert_index = insert_index;
        if from_track == to_track && insert_index > item_index {
            insert_index = insert_index.saturating_sub(1);
        }
        let insert_index = insert_index.min(self.timeline_tracks[to_track].items.len());
        self.timeline_tracks[to_track]
            .items
            .insert(insert_index, moved);
        self.edit_selection.clear();
        self.selection_anchor = Some(TimelineSelectionKey {
            track_index: to_track,
            item_index: insert_index,
        });

        let Some(start_frame) = self
            .sources
            .get(moved.source_index)
            .and_then(|source| source.probe.gops.get(moved.gop_index))
            .map(|gop| gop.start_frame)
        else {
            return;
        };

        self.status = format!(
            "Moved GOP {} from track {} to track {} at position {}",
            moved.gop_index + 1,
            from_track + 1,
            to_track + 1,
            insert_index + 1
        );
        self.activate_source(moved.source_index, ctx);
        self.set_current_frame(start_frame, ctx);
    }

    pub(crate) fn move_selected_gops(
        &mut self,
        to_track: usize,
        insert_index: usize,
        ctx: &egui::Context,
    ) {
        if self.edit_selection.is_empty() || to_track >= self.timeline_tracks.len() {
            return;
        }
        self.sync_active_source_state();
        let selected = self.edit_selection.clone();
        let mut moved = Vec::new();
        let selected_before_insert = selected
            .iter()
            .filter(|key| key.track_index == to_track && key.item_index < insert_index)
            .count();
        self.push_timeline_undo();
        for (track_index, track) in self.timeline_tracks.iter_mut().enumerate() {
            let mut kept = Vec::with_capacity(track.items.len());
            for (item_index, item) in track.items.drain(..).enumerate() {
                if selected.contains(&TimelineSelectionKey {
                    track_index,
                    item_index,
                }) {
                    moved.push(item);
                } else {
                    kept.push(item);
                }
            }
            track.items = kept;
        }
        if moved.is_empty() {
            return;
        }
        let insert_index = insert_index
            .saturating_sub(selected_before_insert)
            .min(self.timeline_tracks[to_track].items.len());
        for (offset, item) in moved.iter().copied().enumerate() {
            self.timeline_tracks[to_track]
                .items
                .insert(insert_index + offset, item);
        }
        self.edit_selection.clear();
        for offset in 0..moved.len() {
            self.edit_selection.insert(TimelineSelectionKey {
                track_index: to_track,
                item_index: insert_index + offset,
            });
        }
        self.selection_anchor = Some(TimelineSelectionKey {
            track_index: to_track,
            item_index: insert_index,
        });
        self.status = format!(
            "Moved {} selected GOP{} to track {}",
            moved.len(),
            if moved.len() == 1 { "" } else { "s" },
            to_track + 1
        );
        if let Some(first) = moved.first().copied() {
            self.seek_item_start(first, ctx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{TimelineItem, TimelineTrack};

    fn app() -> GopComposerApp {
        GopComposerApp {
            timeline_tracks: vec![TimelineTrack {
                name: "Synthetic track".to_owned(),
                color: egui::Color32::WHITE,
                items: (0..3)
                    .map(|gop_index| TimelineItem {
                        source_index: 0,
                        gop_index,
                    })
                    .collect(),
            }],
            ..Default::default()
        }
    }

    #[test]
    fn dragging_first_gop_to_end_preserves_requested_order_and_undo() {
        let mut app = app();
        let original = app.timeline_tracks[0].items.clone();
        app.move_gop_between_tracks(0, 0, 0, 3, &egui::Context::default());
        assert_eq!(
            app.timeline_tracks[0]
                .items
                .iter()
                .map(|i| i.gop_index)
                .collect::<Vec<_>>(),
            vec![1, 2, 0]
        );
        assert_eq!(app.selection_anchor.unwrap().item_index, 2);
        assert_eq!(app.undo_stack[0].tracks[0].items, original);
    }

    #[test]
    fn dropping_last_gop_at_end_does_not_move_it_backwards() {
        let mut app = app();
        let original = app.timeline_tracks[0].items.clone();
        app.move_gop_between_tracks(0, 0, 2, 3, &egui::Context::default());
        assert_eq!(app.timeline_tracks[0].items, original);
    }

    #[test]
    fn dragging_backwards_and_to_another_track_uses_correct_slot() {
        let mut app = app();
        app.move_gop_between_tracks(0, 0, 2, 0, &egui::Context::default());
        assert_eq!(
            app.timeline_tracks[0]
                .items
                .iter()
                .map(|i| i.gop_index)
                .collect::<Vec<_>>(),
            vec![2, 0, 1]
        );
        let mut other = app.timeline_tracks[0].clone();
        other.items.clear();
        app.timeline_tracks.push(other);
        app.move_gop_between_tracks(0, 1, 0, 0, &egui::Context::default());
        assert_eq!(app.timeline_tracks[0].items.len(), 2);
        assert_eq!(app.timeline_tracks[1].items[0].gop_index, 2);
    }
}
