//! Compact object-page chrome shared with the remaining object pages.
use crate::{state::VmTab, widgets};
use egui::{Align, Layout, RichText, Ui};
use xen_core::workspace::ConnectionView;

pub enum Selection {
    Vm,
    Template { has_provision_xml: bool },
    Snapshot,
    Host { live: bool, usb: bool },
    Pool,
    Sr,
    Vdi,
    Home,
    Group,
    Network,
}

pub fn tabs_for(selection: Selection) -> Vec<VmTab> {
    let mut tabs = match selection {
        Selection::Vm => VmTab::ALL[..7].to_vec(),
        Selection::Template {
            has_provision_xml: false,
        } => vec![
            VmTab::General,
            VmTab::Memory,
            VmTab::Storage,
            VmTab::Network,
        ],
        Selection::Template {
            has_provision_xml: true,
        }
        | Selection::Snapshot => vec![VmTab::General, VmTab::Memory, VmTab::Network],
        Selection::Host { live: false, .. } | Selection::Vdi => vec![VmTab::General],
        Selection::Host { live: true, usb } => {
            let mut tabs = vec![
                VmTab::General,
                VmTab::Memory,
                VmTab::Storage,
                VmTab::Network,
                VmTab::Nics,
                VmTab::Gpu,
            ];
            if usb {
                tabs.push(VmTab::Usb);
            }
            tabs.extend([VmTab::Console, VmTab::Performance]);
            tabs
        }
        Selection::Pool => vec![
            VmTab::General,
            VmTab::Memory,
            VmTab::Storage,
            VmTab::Network,
            VmTab::Gpu,
        ],
        Selection::Sr => vec![VmTab::General, VmTab::Storage],
        Selection::Home => vec![VmTab::Home],
        Selection::Group | Selection::Network => vec![],
    };
    tabs.push(VmTab::Search);
    tabs
}

pub(super) fn title_strip(
    ui: &mut Ui,
    connection: &ConnectionView,
    name: &str,
    icon: super::sidebar::TreeIcon,
    power_state: &str,
) {
    let settings = match connection {
        ConnectionView::Online { settings, .. }
        | ConnectionView::Stale { settings, .. }
        | ConnectionView::Offline { settings, .. } => settings,
    };
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(16.0), egui::Sense::hover());
        super::sidebar::paint_icon(ui.painter(), rect, icon, crate::theme::muted_fg());
        widgets::dot(ui, widgets::power_style(power_state).1, 6.0);
        ui.label(RichText::new(name).strong());
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(format!("Logged in as: {}", settings.username));
        });
    });
    ui.separator();
}

pub fn tab_strip(ui: &mut Ui, tabs: &[VmTab], selected: &mut VmTab) {
    let mut index = tabs.iter().position(|tab| tab == selected).unwrap_or(0);
    let labels: Vec<_> = tabs.iter().map(|tab| tab.label()).collect();
    widgets::tab_bar(ui, &labels, &mut index);
    *selected = tabs[index];
}

pub fn value_row(ui: &mut Ui, label: &str, value: &str) {
    ui.spacing_mut().item_spacing.y = 0.0;
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 22.0), egui::Sense::hover());
    ui.painter().text(
        rect.left_center(),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(11.0),
        crate::theme::muted_fg(),
    );
    let value_rect = egui::Rect::from_min_max(rect.min + egui::vec2(200.0, 0.0), rect.max);
    ui.painter().with_clip_rect(value_rect).text(
        value_rect.left_center(),
        egui::Align2::LEFT_CENTER,
        value,
        egui::FontId::proportional(11.0),
        crate::theme::fg(),
    );
    let response = ui.interact(
        value_rect,
        ui.scope_id().with(("value", label)),
        egui::Sense::click(),
    );
    response.on_hover_text(value).context_menu(|ui| {
        if ui.button("Copy").clicked() {
            ui.ctx().copy_text(value.to_owned());
            ui.close();
        }
    });
}
