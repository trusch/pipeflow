//! Filter panel UI.
//!
//! Provides controls for filtering the graph display.

use crate::domain::filters::{FilterPredicate, FilterSet};
use crate::domain::graph::{MediaClass, PortDirection};
use crate::ui::help_texts::help_button;
use crate::ui::theme::Theme;
use egui::Ui;

/// Filter panel component.
pub struct FilterPanel;

impl FilterPanel {
    /// Shows the filter panel.
    pub fn show(ui: &mut Ui, filters: &mut FilterSet, _theme: &Theme) -> FilterPanelResponse {
        let mut response = FilterPanelResponse::default();

        // Search box
        ui.horizontal(|ui| {
            ui.label("Focus:");
            let mut search = filters.search.clone().unwrap_or_default();
            if ui.text_edit_singleline(&mut search).changed() {
                filters.set_search(if search.is_empty() {
                    None
                } else {
                    Some(search)
                });
                response.changed = true;
            }
            help_button(ui, "filters", "search_filter");
        });

        ui.separator();

        // Quick filters
        ui.horizontal(|ui| {
            ui.label("Quick focus:");
            help_button(ui, "filters", "media_type_filters");
        });

        ui.horizontal_wrapped(|ui| {
            response.changed |= Self::toggle_chip(ui, filters, "Audio", FilterPredicate::AudioOnly);
            response.changed |= Self::toggle_chip(ui, filters, "Video", FilterPredicate::VideoOnly);
            response.changed |= Self::toggle_chip(ui, filters, "MIDI", FilterPredicate::MidiOnly);
            response.changed |=
                Self::toggle_chip(ui, filters, "Active", FilterPredicate::ActiveOnly);
        });

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Direction:");
            help_button(ui, "filters", "direction_filters");
        });

        ui.horizontal_wrapped(|ui| {
            response.changed |= Self::toggle_chip(
                ui,
                filters,
                "Inputs",
                FilterPredicate::Direction(PortDirection::Input),
            );
            response.changed |= Self::toggle_chip(
                ui,
                filters,
                "Outputs",
                FilterPredicate::Direction(PortDirection::Output),
            );
        });

        ui.separator();

        // Media class filters
        ui.collapsing("Media Class", |ui| {
            for media_class in Self::common_media_classes() {
                let predicate = FilterPredicate::MediaClass(media_class.clone());
                let is_active = filters.include.contains(&predicate);

                let mut active = is_active;
                if ui
                    .checkbox(&mut active, media_class.display_name())
                    .changed()
                {
                    if active {
                        filters.add_include(predicate);
                    } else {
                        filters.remove_include(&predicate);
                    }
                    response.changed = true;
                }
            }
        });

        ui.separator();

        // Clear filters
        ui.horizontal(|ui| {
            if ui.button("Clear Focus").clicked() {
                filters.clear();
                response.changed = true;
            }

            if !filters.is_empty() {
                ui.label(format!("({} active)", Self::count_active(filters)));
            }
        });

        // Filter description
        if !filters.is_empty() {
            ui.separator();
            ui.label(filters.description());
        }

        response
    }

    /// Shows a toggle chip for a quick filter.
    fn toggle_chip(
        ui: &mut Ui,
        filters: &mut FilterSet,
        label: &str,
        predicate: FilterPredicate,
    ) -> bool {
        let is_active = filters.include.contains(&predicate);

        let button = if is_active {
            egui::Button::new(label).fill(ui.style().visuals.selection.bg_fill)
        } else {
            egui::Button::new(label)
        };

        if ui.add(button).clicked() {
            if is_active {
                filters.remove_include(&predicate);
            } else {
                filters.add_include(predicate);
            }
            return true;
        }

        false
    }

    /// Returns common media classes for filtering.
    fn common_media_classes() -> Vec<MediaClass> {
        vec![
            MediaClass::AudioSource,
            MediaClass::AudioSink,
            MediaClass::StreamOutputAudio,
            MediaClass::StreamInputAudio,
            MediaClass::VideoSource,
            MediaClass::VideoSink,
            MediaClass::MidiSource,
            MediaClass::MidiSink,
        ]
    }

    /// Counts active filters.
    fn count_active(filters: &FilterSet) -> usize {
        filters.include.len() + filters.exclude.len() + if filters.search.is_some() { 1 } else { 0 }
    }
}

/// Response from the filter panel.
#[derive(Debug, Default)]
pub struct FilterPanelResponse {
    /// Whether filters were changed
    pub changed: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_common_media_classes() {
        let classes = FilterPanel::common_media_classes();
        assert!(!classes.is_empty());
        assert!(classes.contains(&MediaClass::AudioSink));
    }
}
