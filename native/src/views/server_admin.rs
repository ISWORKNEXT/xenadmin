//! Upstream server configuration dialogs.
use crate::{
    state::{UiAction, UiState},
    theme, widgets,
};
use xen_core::server_admin::{ServerChange, ServerInfo};
#[derive(Clone, Copy, PartialEq)]
pub enum Page {
    Memory,
    Password,
    Ip,
    CrashDumps,
    Backup,
    StatusReport,
    Restore,
    NewPool,
    JoinPool,
    AddMember,
    Eject,
    Standalone,
    RotateSecret,
}
impl Page {
    pub fn title(self) -> &'static str {
        match self {
            Self::NewPool => "New Pool",
            Self::AddMember => "Add Server to Pool",
            Self::JoinPool => "Add Server to Pool",
            Self::Eject => "Remove Server from Pool",
            Self::Standalone => "Make into standalone server",
            Self::RotateSecret => "Rotate Pool Secret",
            Self::StatusReport => "Server Status Report",
            Self::Backup => "Back Up Server",
            Self::Restore => "Restore Server From Backup",
            Self::Memory => "Control Domain Memory",
            Self::Password => "Change Server Password",
            Self::Ip => "IP Address Configuration",
            Self::CrashDumps => "Remove Crash Dumps",
        }
    }
}
pub struct Dialog {
    pub connection_id: String,
    pub host_uuid: String,
    pub page: Page,
    pub info: Option<ServerInfo>,
    pub memory: String,
    pub path: String,
    pub name: String,
    pub address: String,
    pub username: String,
    pub member: String,
    pub candidates: Vec<(String, String)>,
    pub coordinator: String,
    pub members: Vec<String>,
    pub old: String,
    pub password: String,
    pub confirm: String,
    pub interface: usize,
    pub acknowledged: bool,
    pub busy: bool,
    pub error: Option<String>,
}
impl Dialog {
    pub fn new(connection_id: String, host_uuid: String, page: Page) -> Self {
        Self {
            connection_id: connection_id.clone(),
            host_uuid,
            page,
            info: None,
            memory: String::new(),
            path: String::new(),
            name: String::new(),
            address: String::new(),
            username: "root".into(),
            member: String::new(),
            candidates: Vec::new(),
            coordinator: connection_id.clone(),
            members: Vec::new(),
            old: String::new(),
            password: String::new(),
            confirm: String::new(),
            interface: 0,
            acknowledged: false,
            busy: false,
            error: None,
        }
    }
    pub fn change(&self) -> Result<ServerChange, String> {
        match self.page {
            Page::AddMember => Err("Select members using the pool dialog.".into()),
            Page::NewPool => Ok(ServerChange::NewPool(self.name.clone())),
            Page::JoinPool => {
                if !self.acknowledged {
                    return Err("Confirm joining this server to the destination pool.".into());
                }
                Ok(ServerChange::JoinPool {
                    address: self.address.clone(),
                    username: self.username.clone(),
                    password: self.password.clone(),
                })
            }
            Page::Eject => {
                if self.member.is_empty() {
                    return Err("Select a member server.".into());
                }
                if !self.acknowledged {
                    return Err("Confirm removing the selected server.".into());
                }
                Ok(ServerChange::Eject(self.member.clone()))
            }
            Page::Standalone => {
                if !self.acknowledged {
                    return Err("Confirm making this server standalone.".into());
                }
                Ok(ServerChange::Standalone)
            }
            Page::RotateSecret => {
                if !self.acknowledged {
                    return Err("Confirm rotating the pool secret.".into());
                }
                Ok(ServerChange::RotateSecret)
            }
            Page::Backup | Page::Restore | Page::StatusReport => {
                if self.path.trim().is_empty() {
                    return Err("Select a backup file.".into());
                }
                if self.page == Page::Restore && !self.acknowledged {
                    return Err("Confirm restoring the server backup.".into());
                }
                Ok(if self.page == Page::StatusReport {
                    ServerChange::StatusReport(self.path.clone().into())
                } else if self.page == Page::Backup {
                    ServerChange::Backup(self.path.clone().into())
                } else {
                    ServerChange::Restore(self.path.clone().into())
                })
            }
            Page::Memory if !self.acknowledged => {
                Err("Confirm changing control domain memory and rebooting the server.".into())
            }
            Page::Memory => Ok(ServerChange::Memory(
                self.memory.parse().map_err(|_| "Enter memory in MiB.")?,
            )),
            Page::Password => {
                if self.password != self.confirm {
                    return Err("The new passwords do not match.".into());
                }
                Ok(ServerChange::Password {
                    old: self.old.clone(),
                    new: self.password.clone(),
                })
            }
            Page::Ip => {
                let pif = self
                    .info
                    .as_ref()
                    .and_then(|i| i.interfaces.get(self.interface))
                    .ok_or("Select an interface.")?;
                if pif.management && !self.acknowledged {
                    return Err("Acknowledge the management connection change.".into());
                }
                xen_core::server_admin::validate_ip(pif).map_err(|e| e.to_string())?;
                Ok(ServerChange::Ip(pif.clone()))
            }
            Page::CrashDumps => {
                if !self.acknowledged {
                    return Err("Confirm removal of the server's crash dumps.".into());
                }
                Ok(ServerChange::RemoveCrashDumps)
            }
        }
    }
}
pub fn show(ctx: &egui::Context, state: &mut UiState, actions: &mut Vec<UiAction>) {
    let Some(d) = state.server_admin.as_mut() else {
        return;
    };
    let response = egui::Modal::new(egui::Id::unique("server-admin"))
        .frame(theme::modal_frame())
        .show(ctx, |ui| {
            ui.set_width(560.0);
            ui.heading(d.page.title());
            if let Some(info) = d.info.as_mut() {
                ui.label(&info.name);
                ui.separator();
                ui.add_enabled_ui(!d.busy, |ui| match d.page {
                    Page::NewPool | Page::AddMember=> {
                        if d.page==Page::NewPool {
                            ui.label("Name");ui.text_edit_singleline(&mut d.name);
                            ui.label("Coordinator");
                            egui::ComboBox::from_id_salt("new-pool-coordinator").selected_text(d.candidates.iter().find(|(id,_)|id==&d.coordinator).map(|(_,name)|name.as_str()).unwrap_or(&info.name)).show_ui(ui,|ui|{for (id,name) in &d.candidates {ui.selectable_value(&mut d.coordinator,id.clone(),name);}});
                        }else{ui.label(format!("Coordinator: {}",info.name));}
                        ui.label("Members");
                        for (id,name) in d.candidates.iter().filter(|(id,_)|id!=&d.coordinator) {
                            let mut selected=d.members.contains(id);
                            if ui.checkbox(&mut selected,name).changed(){if selected {d.members.push(id.clone());}else{d.members.retain(|member|member!=id);}}
                        }
                    }
                    Page::JoinPool=> {
                        ui.label("Destination coordinator address");ui.text_edit_singleline(&mut d.address);ui.label("Username");ui.text_edit_singleline(&mut d.username);ui.label("Password");ui.add(egui::TextEdit::singleline(&mut d.password).password(true));
                        ui.label("Joining changes this server's pool membership and reconnect address.");ui.checkbox(&mut d.acknowledged,"Join this server to the selected pool");
                    }
                    Page::Eject=> {
                        egui::ComboBox::from_id_salt("eject-member").selected_text(info.hosts.iter().find(|(id,_,_)|id==&d.member).map(|(_,name,_)|name.as_str()).unwrap_or("Select a member")).show_ui(ui,|ui|{for (id,name,master) in &info.hosts {if !master {ui.selectable_value(&mut d.member,id.clone(),name);}}});
                        ui.label("Removing a server disconnects it from the pool and may remove its local VM metadata.");ui.checkbox(&mut d.acknowledged,"Remove this server from the pool");
                    }
                    Page::Standalone=>{ui.label("Disconnect all member servers from this pool and make the coordinator standalone.");ui.checkbox(&mut d.acknowledged,"Make this server standalone");}
                    Page::RotateSecret=>{ui.label("Rotate the authentication secret shared by all servers in this pool.");ui.checkbox(&mut d.acknowledged,"Rotate the pool secret");}
                    Page::Backup | Page::Restore | Page::StatusReport => {
                        ui.label(if d.page == Page::StatusReport { "Destination status report file (tar.bz2)" } else if d.page == Page::Backup {
                            "Destination backup file"
                        } else {
                            "Server backup file"
                        });
                        ui.text_edit_singleline(&mut d.path);
                        if ui.button("Browse…").clicked(){actions.push(UiAction::BrowseServerBackup);}
                        if d.page == Page::Restore {
                            ui.checkbox(
                                &mut d.acknowledged,
                                "Restore this server from the selected backup",
                            );
                        }
                    }
                    Page::Memory => {
                        ui.label("Memory allocated to the control domain (dom0)");
                        ui.add_enabled_ui(info.maintenance,|ui| {
                            ui.horizontal(|ui| {ui.text_edit_singleline(&mut d.memory);ui.label("MiB");});
                            ui.checkbox(&mut d.acknowledged,"Change memory and reboot this server");
                        });
                        if !info.maintenance {ui.label("Enter maintenance mode before changing control domain memory.");}
                        ui.label(format!("Allowed range: {}–{} MiB. Applying this change reboots the server.",info.min_memory_mib,info.max_memory_mib));
                    }
                    Page::Password => {
                        for (label, value) in [
                            ("Current password", &mut d.old),
                            ("New password", &mut d.password),
                            ("Confirm new password", &mut d.confirm),
                        ] {
                            ui.label(label);
                            ui.add(
                                egui::TextEdit::singleline(value)
                                    .password(true)
                                    .desired_width(f32::INFINITY),
                            );
                        }
                    }
                    Page::CrashDumps => {
                        ui.label("Remove all crash dumps stored on this server.");
                        ui.checkbox(&mut d.acknowledged, "Remove these crash dumps permanently");
                    }
                    Page::Ip => {
                        egui::ComboBox::from_id_salt("ip-interface")
                            .selected_text(
                                info.interfaces
                                    .get(d.interface)
                                    .map(|p| p.name.as_str())
                                    .unwrap_or("Select interface"),
                            )
                            .show_ui(ui, |ui| {
                                for (index, pif) in info.interfaces.iter().enumerate() {
                                    ui.selectable_value(
                                        &mut d.interface,
                                        index,
                                        format!(
                                            "{}{}",
                                            pif.name,
                                            if pif.management { " (Management)" } else { "" }
                                        ),
                                    );
                                }
                            });
                        if let Some(pif) = info.interfaces.get_mut(d.interface) {
                            ui.radio_value(
                                &mut pif.mode,
                                "DHCP".into(),
                                "Automatically obtain IP settings using DHCP",
                            );
                            ui.radio_value(&mut pif.mode, "Static".into(), "Use these IP settings");
                            ui.add_enabled_ui(pif.mode == "Static", |ui| {
                                for (label, value) in [
                                    ("IP address", &mut pif.ip),
                                    ("Subnet mask", &mut pif.mask),
                                    ("Gateway", &mut pif.gateway),
                                    ("DNS servers", &mut pif.dns),
                                ] {
                                    ui.label(label);
                                    ui.text_edit_singleline(value);
                                }
                            });
                            if !pif.management {ui.checkbox(&mut pif.management,"Use this interface for management");}
                            if pif.management {
                                ui.colored_label(
                                    theme::red(),
                                    "Changing these settings may disconnect this server.",
                                );
                                ui.checkbox(
                                    &mut d.acknowledged,
                                    "I understand the management connection will change",
                                );
                            }
                        }
                    }
                });
            } else {
                ui.spinner();
                ui.label("Reading server configuration…");
            }
            if let Some(error) = &d.error {
                ui.colored_label(theme::red(), error);
            }
            ui.separator();
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(!d.busy, egui::Button::new("Cancel"))
                    .clicked()
                {
                    actions.push(UiAction::CloseServerAdmin);
                }
                if ui
                    .add_enabled(!d.busy && d.info.is_some(), widgets::primary_button("OK"))
                    .clicked()
                {
                    actions.push(UiAction::SaveServerAdmin);
                }
            });
        });
    if response.should_close() && !d.busy {
        actions.push(UiAction::CloseServerAdmin);
    }
}
