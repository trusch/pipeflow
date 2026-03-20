//! Status messaging and persistent issue rendering.

use super::{FeedbackLevel, PipeflowApp};
use crate::app::types::PersistentIssue;

impl PipeflowApp {
    pub(super) fn set_status_message(&mut self, message: impl Into<String>, is_error: bool) {
        self.components.status_message =
            Some((message.into(), std::time::Instant::now(), is_error));
    }

    pub(super) fn push_persistent_issue(
        &mut self,
        key: impl Into<String>,
        level: FeedbackLevel,
        summary: impl Into<String>,
        detail: Option<String>,
    ) {
        let key = key.into();
        let summary = summary.into();
        if let Some(existing) = self
            .components
            .persistent_issues
            .iter_mut()
            .find(|issue| issue.key == key)
        {
            existing.level = level;
            existing.summary = summary;
            existing.detail = detail;
            return;
        }
        self.components.persistent_issues.push(PersistentIssue {
            key,
            level,
            summary,
            detail,
        });
    }

    pub(super) fn resolve_persistent_issue(&mut self, key: &str) {
        self.components
            .persistent_issues
            .retain(|issue| issue.key != key);
    }

    /// Renders transient confirmations plus dismissible persistent warnings and errors.
    pub(super) fn render_status_bar(&mut self, ctx: &egui::Context) {
        const STATUS_DURATION: std::time::Duration = std::time::Duration::from_secs(5);

        if let Some((_, created, _)) = &self.components.status_message {
            if created.elapsed() >= STATUS_DURATION {
                self.components.status_message = None;
            }
        }

        if !self.components.persistent_issues.is_empty() {
            egui::TopBottomPanel::bottom("persistent_feedback")
                .resizable(false)
                .show(ctx, |ui| {
                    let mut dismiss_key = None;
                    for issue in &self.components.persistent_issues {
                        let (accent, icon) = match issue.level {
                            FeedbackLevel::Warning => (
                                egui::Color32::from_rgb(255, 200, 100),
                                egui_phosphor::regular::WARNING,
                            ),
                            FeedbackLevel::Error => (
                                egui::Color32::from_rgb(255, 120, 120),
                                egui_phosphor::regular::WARNING_OCTAGON,
                            ),
                        };

                        egui::Frame::NONE
                            .fill(egui::Color32::from_rgba_unmultiplied(
                                accent.r(),
                                accent.g(),
                                accent.b(),
                                18,
                            ))
                            .stroke(egui::Stroke::new(1.0, accent))
                            .corner_radius(8)
                            .inner_margin(egui::Margin::same(8))
                            .show(ui, |ui| {
                                ui.horizontal_top(|ui| {
                                    ui.label(egui::RichText::new(icon).color(accent));
                                    ui.vertical(|ui| {
                                        ui.label(
                                            egui::RichText::new(&issue.summary)
                                                .strong()
                                                .color(accent),
                                        );
                                        if let Some(detail) = &issue.detail {
                                            ui.label(detail);
                                        }
                                    });
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Min),
                                        |ui| {
                                            if ui.small_button("Dismiss").clicked() {
                                                dismiss_key = Some(issue.key.clone());
                                            }
                                        },
                                    );
                                });
                            });
                        ui.add_space(4.0);
                    }
                    if let Some(key) = dismiss_key {
                        self.resolve_persistent_issue(&key);
                    }
                });
        }

        if let Some((msg, _, is_error)) = &self.components.status_message {
            egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
                let (color, icon) = if *is_error {
                    (
                        egui::Color32::from_rgb(255, 120, 120),
                        egui_phosphor::regular::WARNING,
                    )
                } else {
                    (
                        egui::Color32::from_rgb(120, 220, 150),
                        egui_phosphor::regular::CHECK,
                    )
                };
                egui::Frame::NONE
                    .fill(egui::Color32::from_rgba_unmultiplied(
                        color.r(),
                        color.g(),
                        color.b(),
                        18,
                    ))
                    .inner_margin(egui::Margin::symmetric(10, 8))
                    .show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(egui::RichText::new(icon).color(color));
                            ui.colored_label(color, msg);
                        });
                    });
            });
        }
    }
}
