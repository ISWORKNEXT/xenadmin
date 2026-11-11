//! Shared wizard window. Pages own their data and validation; the frame owns navigation.
use egui::{Align, Layout, RichText};

use crate::{theme, widgets};

#[derive(Default)]
pub struct WizardState {
    pub step: usize,
    pub confirm_cancel: bool,
}

#[derive(Clone, Copy)]
pub struct Step {
    pub title: &'static str,
    pub description: &'static str,
}

#[derive(PartialEq, Eq)]
pub enum Outcome {
    None,
    Cancel,
    Finish,
}

pub struct Frame<'a> {
    pub id: &'static str,
    pub title: &'static str,
    pub steps: &'a [Step],
    pub busy: bool,
    pub dirty: bool,
    pub error: Option<&'a str>,
}

impl Frame<'_> {
    pub fn show(
        &self,
        ctx: &egui::Context,
        state: &mut WizardState,
        mut page: impl FnMut(&mut egui::Ui, usize) -> Option<&'static str>,
    ) -> Outcome {
        let mut outcome = Outcome::None;
        let mut cancel = false;
        let step = state.step.min(self.steps.len() - 1);
        let current = self.steps[step];
        let confirm_cancel = state.confirm_cancel;
        let enter =
            ctx.input(|input| input.key_pressed(egui::Key::Enter) && input.modifiers.is_none());
        let response = egui::Modal::new(egui::Id::unique(self.id))
            .frame(theme::modal_frame())
            .show(ctx, |ui| {
                ui.set_width(828.0);
                ui.heading(self.title);
                ui.separator();
                let mut validation = None;
                ui.horizontal_top(|ui| {
                    ui.set_height(466.0);
                    ui.allocate_ui_with_layout(
                        egui::vec2(200.0, 466.0),
                        Layout::top_down(Align::Min),
                        |ui| {
                            ui.set_width(200.0);
                            for (index, item) in self.steps.iter().enumerate() {
                                ui.add_space(9.0);
                                let text = RichText::new(item.title);
                                let text = if index == step {
                                    text.strong().color(theme::accent())
                                } else {
                                    text
                                };
                                ui.add_enabled(index <= step, egui::Label::new(text));
                            }
                        },
                    );
                    ui.separator();
                    ui.vertical(|ui| {
                        ui.set_width(600.0);
                        ui.heading(current.title);
                        ui.label(RichText::new(current.description).color(theme::muted_fg()));
                        ui.separator();
                        egui::ScrollArea::vertical()
                            .id_salt((self.id, step))
                            .max_height(385.0)
                            .show(ui, |ui| {
                                ui.add_enabled_ui(!self.busy && !confirm_cancel, |ui| {
                                    validation = page(ui, step);
                                });
                            });
                    });
                });
                if let Some(error) = self.error {
                    ui.colored_label(theme::red(), error);
                }
                if let Some(error) = validation {
                    ui.label(RichText::new(error).color(theme::muted_fg()));
                }
                ui.separator();
                if state.confirm_cancel {
                    ui.horizontal(|ui| {
                        ui.label("Discard the information entered in this wizard?");
                        if ui.button("Discard").clicked() {
                            outcome = Outcome::Cancel;
                        }
                        if ui.button("Continue editing").clicked() {
                            state.confirm_cancel = false;
                        }
                    });
                } else {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        cancel = ui
                            .add_enabled(!self.busy, egui::Button::new("Cancel"))
                            .clicked();
                        let last = step == self.steps.len() - 1;
                        let next = ui
                            .add_enabled(
                                !self.busy && validation.is_none(),
                                widgets::primary_button(if last { "Finish" } else { "Next >" }),
                            )
                            .clicked();
                        let previous = ui
                            .add_enabled(!self.busy && step > 0, egui::Button::new("< Previous"))
                            .clicked();
                        if !self.busy && validation.is_none() && (next || enter) {
                            if last {
                                outcome = Outcome::Finish;
                            } else {
                                state.step = step + 1;
                            }
                        } else if previous {
                            state.step = step - 1;
                        }
                        if self.busy {
                            ui.spinner();
                        }
                    });
                }
            });
        if !self.busy && (cancel || response.should_close()) {
            if self.dirty {
                state.confirm_cancel = true;
            } else {
                outcome = Outcome::Cancel;
            }
        }
        outcome
    }
}
