use super::wizard::{Frame, Outcome, Step};
use crate::{
    state::{UiAction, UiState},
    widgets,
};
const STEPS: &[Step] = &[
    Step {
        title: "Type",
        description: "Select the type of network to create.",
    },
    Step {
        title: "Name",
        description: "Enter a name and description.",
    },
    Step {
        title: "Details",
        description: "Configure this network.",
    },
    Step {
        title: "Finish",
        description: "Review the network before creating it.",
    },
];
pub fn show(ctx: &egui::Context, state: &mut UiState, actions: &mut Vec<UiAction>) {
    let Some(d) = state.network_create_dialog.as_mut() else {
        return;
    };
    let pifs = &state.pif_catalog;
    let mut wizard = std::mem::take(&mut d.wizard);
    let outcome = Frame {
        id: "new-network-wizard",
        title: "New Network",
        steps: STEPS,
        busy: d.submitting,
        dirty: !d.name.is_empty(),
        error: d.error.as_deref(),
    }
    .show(ctx, &mut wizard, |ui, step| {
        match step {
            0 => {
                for (kind, label) in [
                    ("vlan", "External Network"),
                    ("internal", "Single-Server Private Network"),
                    ("chin", "Cross-Server Private Network"),
                    ("bond", "Bonded Network"),
                ] {
                    ui.radio_value(&mut d.kind, kind.into(), label);
                }
                ui.add_enabled_ui(pifs.iter().any(|p| p.sriov_candidate()), |ui| {
                    ui.radio_value(&mut d.kind, "sriov".into(), "SR-IOV Network");
                });
                ui.label("Private networks isolate virtual machines from external networks.");
                if state.pif_catalog_loading {
                    ui.spinner();
                    return Some("Reading network interfaces…");
                }
                if state.pif_catalog_error.is_some() {
                    return Some("Unable to read network interfaces.");
                }
            }
            1 => {
                widgets::field_label(ui, "Name");
                ui.add(
                    egui::TextEdit::singleline(&mut d.name)
                        .char_limit(255)
                        .desired_width(f32::INFINITY),
                );
                widgets::field_label(ui, "Description");
                ui.add(
                    egui::TextEdit::multiline(&mut d.description)
                        .char_limit(1024)
                        .desired_width(f32::INFINITY),
                );
                if d.name.trim().is_empty() {
                    return Some("Enter a network name.");
                }
            }
            2 => {
                ui.horizontal(|ui| {
                    ui.label("MTU");
                    ui.text_edit_singleline(&mut d.mtu);
                });
                ui.checkbox(
                    &mut d.autoplug,
                    "Automatically add this network to new virtual machines",
                );
                if d.kind == "bond" {
                    ui.label("Select two or more NICs on the same server:");
                    for pif in pifs.iter().filter(|p| p.bond_candidate()) {
                        let mut selected = d.members.contains(&pif.uuid);
                        if ui
                            .checkbox(&mut selected, format!("{} — {}", pif.host_name, pif.device))
                            .changed()
                        {
                            if selected {
                                d.members.push(pif.uuid.clone());
                            } else {
                                d.members.retain(|id| id != &pif.uuid);
                            }
                        }
                    }
                    for (value, label) in [
                        ("balance-slb", "Active-active"),
                        ("active-backup", "Active-passive"),
                        ("lacp", "LACP"),
                    ] {
                        ui.radio_value(&mut d.mode, value.into(), label);
                    }
                    if d.members.len() < 2 {
                        return Some("Select at least two NICs.");
                    }
                } else if matches!(d.kind.as_str(), "chin" | "sriov") {
                    ui.label("Select a transport NIC on each server:");
                    for pif in pifs.iter().filter(|p| {
                        if d.kind == "chin" {
                            p.chin_candidate()
                        } else {
                            p.sriov_candidate()
                        }
                    }) {
                        let mut selected = d.members.contains(&pif.uuid);
                        if ui
                            .checkbox(&mut selected, format!("{} — {}", pif.host_name, pif.device))
                            .changed()
                        {
                            if selected {
                                d.members.push(pif.uuid.clone());
                            } else {
                                d.members.retain(|id| id != &pif.uuid);
                            }
                        }
                    }
                    if d.kind == "chin" {
                        ui.radio_value(&mut d.protocol, "gre".into(), "GRE");
                        ui.radio_value(&mut d.protocol, "vxlan".into(), "VXLAN");
                    }
                    if d.members.is_empty() {
                        return Some("Select at least one NIC.");
                    }
                } else if d.kind != "internal" {
                    ui.label("NIC");
                    egui::ComboBox::from_id_salt("network-interface")
                        .selected_text(
                            d.pif_uuid
                                .as_ref()
                                .and_then(|id| pifs.iter().find(|p| &p.uuid == id))
                                .map(|p| format!("{} — {}", p.host_name, p.device))
                                .unwrap_or("Select a NIC".into()),
                        )
                        .show_ui(ui, |ui| {
                            for p in pifs.iter().filter(|p| {
                                if d.kind == "chin" {
                                    p.chin_candidate()
                                } else if d.kind == "sriov" {
                                    p.sriov_candidate()
                                } else {
                                    p.vlan_candidate()
                                }
                            }) {
                                ui.selectable_value(
                                    &mut d.pif_uuid,
                                    Some(p.uuid.clone()),
                                    format!("{} — {}", p.host_name, p.device),
                                );
                            }
                        });
                    if d.kind == "vlan" {
                        ui.label("VLAN (0–4094)");
                        ui.text_edit_singleline(&mut d.tag);
                        if !d.tag.parse::<u16>().is_ok_and(|tag| tag <= 4094) {
                            return Some("Enter a VLAN from 0 to 4094.");
                        }
                    }
                    if d.kind == "chin" {
                        ui.radio_value(&mut d.protocol, "gre".into(), "GRE");
                        ui.radio_value(&mut d.protocol, "vxlan".into(), "VXLAN");
                    }
                    if d.pif_uuid.is_none() {
                        return Some("Select a NIC.");
                    }
                }
                if !d
                    .mtu
                    .parse::<u64>()
                    .is_ok_and(|n| (1500..=9000).contains(&n))
                {
                    return Some("Enter an MTU from 1500 to 9000.");
                }
            }
            _ => {
                ui.label(format!("Name: {}", d.name));
                ui.label(format!(
                    "Type: {}",
                    match d.kind.as_str() {
                        "internal" => "Single-Server Private Network",
                        "vlan" => "External Network",
                        "bond" => "Bonded Network",
                        "chin" => "Cross-Server Private Network",
                        _ => "SR-IOV Network",
                    }
                ));
                ui.label(format!("MTU: {}", d.mtu));
                ui.label(&d.description);
            }
        }
        None
    });
    d.wizard = wizard;
    match outcome {
        Outcome::Cancel => actions.push(UiAction::CloseNetworkCreate),
        Outcome::Finish => actions.push(UiAction::SaveNetwork),
        Outcome::None => {}
    }
}
