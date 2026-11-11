use crate::state::{UiAction, UiState};
use egui::Ui;
use xen_core::workspace::WorkspaceView;

pub fn show(
    ui: &mut Ui,
    state: &mut UiState,
    workspace: &WorkspaceView,
    actions: &mut Vec<UiAction>,
) {
    super::infrastructure_page::show(ui, state, workspace, actions);
}
