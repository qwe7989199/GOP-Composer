use std::collections::{BTreeMap, BTreeSet};

use eframe::egui;

use super::{GopComposerApp, TimelineItem, TimelineSelectionKey};

#[derive(Clone, Debug)]
pub(crate) struct TimecodeApplyPlan {
    replacement_source_index: usize,
    replacement_name: String,
    replacement_timecode: String,
    rate_num: i32,
    rate_den: i32,
    replacements: Vec<TimecodeReplacement>,
    unmatched_replacement_gops: usize,
}

#[derive(Clone, Copy, Debug)]
struct TimecodeReplacement {
    initial_main_item_index: usize,
    replacement_gop_index: usize,
    absolute_frame: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TimecodePromptAction {
    Apply,
    Cancel,
}

impl GopComposerApp {
    pub(crate) fn queue_timecode_apply_for_source(&mut self, replacement_source_index: usize) {
        if self.main_track_starts_with_source(replacement_source_index) {
            return;
        }
        let Some(replacement) = self.sources.get(replacement_source_index) else {
            return;
        };
        if replacement.probe.timecode.is_none() {
            return;
        }
        let replacement_name = replacement.name.clone();

        let plan = self.build_timecode_apply_plan(replacement_source_index);
        let Some(plan) = plan else {
            self.status = self.text().timecode_no_compatible_main(&replacement_name);
            return;
        };
        if plan.replacements.is_empty() {
            self.status = self.text().timecode_no_match(&plan.replacement_name);
            return;
        }

        let message = self
            .text()
            .timecode_match_detected(&plan.replacement_name, plan.replacements.len());
        if self.pending_timecode_apply.is_none() {
            self.pending_timecode_apply = Some(plan);
        } else {
            self.timecode_apply_queue.push_back(plan);
        }
        self.status = message;
    }

    pub(crate) fn timecode_apply_prompt_ui(&mut self, ctx: &egui::Context) {
        self.promote_next_timecode_apply_plan();
        let Some(plan) = self.pending_timecode_apply.clone() else {
            return;
        };
        let text = self.text();

        let mut open = true;
        let mut action = None;
        egui::Window::new(text.apply_timecode_title())
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .frame(
                egui::Frame::window(&ctx.style())
                    .fill(egui::Color32::from_rgb(248, 252, 255))
                    .stroke(egui::Stroke::new(
                        1.0,
                        egui::Color32::from_rgb(117, 159, 196),
                    ))
                    .corner_radius(egui::CornerRadius::ZERO),
            )
            .show(ctx, |ui| {
                ui.set_min_width(430.0);
                ui.label(text.timecode_contains(
                    &plan.replacement_name,
                    &plan.replacement_timecode,
                    plan.rate_num,
                    plan.rate_den,
                ));
                ui.add_space(6.0);
                ui.label(text.timecode_gops_match(plan.replacements.len()));
                if plan.unmatched_replacement_gops > 0 {
                    ui.label(
                        egui::RichText::new(
                            text.timecode_gops_unmatched(plan.unmatched_replacement_gops),
                        )
                        .color(egui::Color32::from_rgb(190, 73, 96)),
                    );
                }
                if let (Some(first), Some(last)) =
                    (plan.replacements.first(), plan.replacements.last())
                {
                    ui.label(text.absolute_frame_range(first.absolute_frame, last.absolute_frame));
                }
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.button(text.apply()).clicked() {
                        action = Some(TimecodePromptAction::Apply);
                    }
                    if ui.button(text.cancel()).clicked() {
                        action = Some(TimecodePromptAction::Cancel);
                    }
                });
            });

        if !open {
            action = Some(TimecodePromptAction::Cancel);
        }
        match action {
            Some(TimecodePromptAction::Apply) => {
                if let Some(plan) = self.pending_timecode_apply.take() {
                    self.apply_timecode_plan(plan, ctx);
                }
                self.promote_next_timecode_apply_plan();
            }
            Some(TimecodePromptAction::Cancel) => {
                if let Some(plan) = self.pending_timecode_apply.take() {
                    self.status = self.text().skipped_timecode_apply(&plan.replacement_name);
                }
                self.promote_next_timecode_apply_plan();
            }
            None => {}
        }
    }

    fn build_timecode_apply_plan(
        &self,
        replacement_source_index: usize,
    ) -> Option<TimecodeApplyPlan> {
        let replacement = self.sources.get(replacement_source_index)?;
        let replacement_timecode = replacement.probe.timecode.as_ref()?;
        let mut main_by_timecode = BTreeMap::new();
        for (item_index, item) in self.timeline_tracks.first()?.items.iter().enumerate() {
            let Some(absolute_frame) = self.timeline_item_absolute_frame(
                *item,
                replacement_timecode.rate_num,
                replacement_timecode.rate_den,
            ) else {
                continue;
            };
            main_by_timecode.entry(absolute_frame).or_insert(item_index);
        }
        if main_by_timecode.is_empty() {
            return None;
        }

        let mut used_main_items = BTreeSet::new();
        let mut replacements = Vec::new();
        let mut unmatched_replacement_gops = 0usize;
        for replacement_gop in &replacement.probe.gops {
            let absolute_frame = replacement_timecode.start_frame + replacement_gop.start_frame;
            if let Some(&main_item_index) = main_by_timecode.get(&absolute_frame)
                && used_main_items.insert(main_item_index)
            {
                replacements.push(TimecodeReplacement {
                    initial_main_item_index: main_item_index,
                    replacement_gop_index: replacement_gop.index,
                    absolute_frame,
                });
            } else {
                unmatched_replacement_gops += 1;
            }
        }

        Some(TimecodeApplyPlan {
            replacement_source_index,
            replacement_name: replacement.name.clone(),
            replacement_timecode: replacement_timecode.value.clone(),
            rate_num: replacement_timecode.rate_num,
            rate_den: replacement_timecode.rate_den,
            replacements,
            unmatched_replacement_gops,
        })
    }

    fn apply_timecode_plan(&mut self, plan: TimecodeApplyPlan, ctx: &egui::Context) {
        let Some(replacement_source) = self.sources.get(plan.replacement_source_index) else {
            self.status = self.text().replacement_source_missing().to_owned();
            return;
        };
        let replacement_gop_count = replacement_source.probe.gops.len();
        if self
            .timeline_tracks
            .first()
            .is_none_or(|track| track.items.is_empty())
        {
            self.status = self.text().main_track_empty().to_owned();
            return;
        }

        self.push_timeline_undo();
        let mut applied = 0usize;
        let mut applied_gops = Vec::new();
        let mut first_item = None;
        let mut selected = BTreeSet::new();
        let mut used_main_items = BTreeSet::new();
        for replacement in plan.replacements {
            if replacement.replacement_gop_index >= replacement_gop_count {
                continue;
            }
            let main_item_index = self
                .main_track_item_index_for_timecode(
                    replacement.absolute_frame,
                    plan.rate_num,
                    plan.rate_den,
                )
                .unwrap_or(replacement.initial_main_item_index);
            if !used_main_items.insert(main_item_index) {
                continue;
            }
            let Some(slot) = self
                .timeline_tracks
                .first_mut()
                .and_then(|track| track.items.get_mut(main_item_index))
            else {
                continue;
            };
            let item = TimelineItem {
                source_index: plan.replacement_source_index,
                gop_index: replacement.replacement_gop_index,
            };
            *slot = item;
            first_item.get_or_insert(item);
            applied_gops.push(replacement.replacement_gop_index);
            selected.insert(TimelineSelectionKey {
                track_index: 0,
                item_index: main_item_index,
            });
            applied += 1;
        }
        self.consume_applied_replacement_gops(plan.replacement_source_index, &applied_gops);

        self.edit_selection = selected;
        self.selection_anchor = self.edit_selection.iter().next().copied();
        if let Some(item) = first_item {
            self.seek_item_start(item, ctx);
        }
        self.status = if applied == 0 {
            self.text()
                .no_timecode_mapped_applied(&plan.replacement_name)
        } else {
            self.text()
                .applied_timecode_mapped(&plan.replacement_name, applied)
        };
    }

    fn consume_applied_replacement_gops(&mut self, source_index: usize, gop_indices: &[usize]) {
        for &gop_index in gop_indices {
            self.remove_one_replacement_track_item(source_index, gop_index);
        }
        let mut kept_tracks = Vec::with_capacity(self.timeline_tracks.len());
        for (track_index, track) in self.timeline_tracks.drain(..).enumerate() {
            if track_index == 0 || !track.items.is_empty() {
                kept_tracks.push(track);
            }
        }
        self.timeline_tracks = kept_tracks;
    }

    fn remove_one_replacement_track_item(&mut self, source_index: usize, gop_index: usize) {
        for track_index in (1..self.timeline_tracks.len()).rev() {
            let Some(item_index) = self.timeline_tracks[track_index]
                .items
                .iter()
                .position(|item| item.source_index == source_index && item.gop_index == gop_index)
            else {
                continue;
            };
            self.timeline_tracks[track_index].items.remove(item_index);
            break;
        }
    }

    fn promote_next_timecode_apply_plan(&mut self) {
        if self.pending_timecode_apply.is_none() {
            self.pending_timecode_apply = self.timecode_apply_queue.pop_front();
        }
    }

    fn main_track_starts_with_source(&self, source_index: usize) -> bool {
        self.timeline_tracks
            .first()
            .and_then(|track| track.items.first())
            .is_some_and(|item| item.source_index == source_index)
    }

    fn main_track_item_index_for_timecode(
        &self,
        absolute_frame: i64,
        rate_num: i32,
        rate_den: i32,
    ) -> Option<usize> {
        self.timeline_tracks
            .first()?
            .items
            .iter()
            .enumerate()
            .find_map(|(item_index, item)| {
                (self.timeline_item_absolute_frame(*item, rate_num, rate_den)? == absolute_frame)
                    .then_some(item_index)
            })
    }

    fn timeline_item_absolute_frame(
        &self,
        item: TimelineItem,
        rate_num: i32,
        rate_den: i32,
    ) -> Option<i64> {
        let source = self.sources.get(item.source_index)?;
        let timecode = source.probe.timecode.as_ref()?;
        if (timecode.rate_num, timecode.rate_den) != (rate_num, rate_den) {
            return None;
        }
        let gop = source.probe.gops.get(item.gop_index)?;
        Some(timecode.start_frame + gop.start_frame)
    }
}
