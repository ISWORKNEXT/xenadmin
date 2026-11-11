use egui::{Align, Layout, RichText, Ui};
use egui_extras::{Column, TableBuilder};
use xen_core::pool_status::{PoolConfigEntry, PoolStatus};
use xen_core::workspace::WorkspaceView;

use crate::state::{
    LoadState, ObjectKey, UiAction, UiState, connection_name, find_connection, is_read_only,
};
use crate::theme;
use crate::theme::*;
use crate::widgets;

pub fn show(
    ui: &mut Ui,
    state: &mut UiState,
    workspace: &WorkspaceView,
    actions: &mut Vec<UiAction>,
) {
    let Some(ObjectKey::PoolStatus { connection_id }) =
        state.selection.as_ref().map(|tab| tab.key.clone())
    else {
        return;
    };
    let Some(connection) = find_connection(workspace, &connection_id) else {
        widgets::empty_state(
            ui,
            "Connection unavailable",
            "The saved connection was removed.",
        );
        return;
    };
    if matches!(state.pool_status_state, LoadState::Idle) {
        actions.push(UiAction::LoadPoolStatus);
    }
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.label(RichText::new("POOL STATUS").size(8.5).color(dim_fg()));
            ui.label(
                RichText::new(connection_name(connection))
                    .size(23.0)
                    .strong(),
            );
            ui.label(
                RichText::new("HA, workload balancing, PVS, VM snapshot schedules and DR")
                    .size(9.5)
                    .color(muted_fg()),
            );
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui
                .add(egui::Button::new(RichText::new("Refresh").size(11.0)))
                .clicked()
            {
                actions.push(UiAction::LoadPoolStatus);
            }
        });
    });
    ui.add_space(10.0);
    match &state.pool_status_state {
        LoadState::Idle | LoadState::Loading => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(
                    RichText::new("Loading pool status…")
                        .size(10.5)
                        .color(muted_fg()),
                );
            });
        }
        LoadState::Error(message) => {
            theme::card_frame().show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new("Could not load pool status").strong());
                ui.label(RichText::new(message).size(10.0).color(red()));
            });
        }
        LoadState::Ready(status) => render(ui, status, actions, is_read_only(connection)),
    }
}

fn render(ui: &mut Ui, status: &PoolStatus, actions: &mut Vec<UiAction>, read_only: bool) {
    summary(ui, status);
    ui.add_space(8.0);
    ui.columns(2, |columns| {
        ha_card(&mut columns[0], status, actions, read_only);
        wlb_card(&mut columns[1], status, actions, read_only);
    });
    ui.add_space(8.0);
    dr_card(ui, status, actions, read_only);
    ui.add_space(8.0);
    pvs_card(ui, status, actions, read_only);
    ui.add_space(8.0);
    sriov_card(ui, status);
    ui.add_space(8.0);
    vmss_card(ui, status, actions, read_only);
}

fn summary(ui: &mut Ui, status: &PoolStatus) {
    ui.columns(5, |columns| {
        let cards = [
            (
                "HA",
                if status.ha.enabled {
                    "Enabled"
                } else {
                    "Disabled"
                },
                if status.ha.enabled { green() } else { amber() },
            ),
            (
                "WLB",
                if status.wlb.enabled {
                    "Enabled"
                } else {
                    "Disabled"
                },
                if status.wlb.enabled { green() } else { amber() },
            ),
            ("PVS sites", &status.pvs.sites.len().to_string(), accent()),
            ("VMSS", &status.vmss.len().to_string(), purple()),
            ("DR tasks", &status.dr.len().to_string(), cyan()),
        ];
        for (index, (label, value, color)) in cards.into_iter().enumerate() {
            theme::card_frame()
                .inner_margin(egui::Margin::same(12))
                .show(&mut columns[index], |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        widgets::dot(ui, color, 7.0);
                        ui.label(RichText::new(label).size(10.0).color(muted_fg()));
                    });
                    ui.label(RichText::new(value).size(16.0).strong());
                });
        }
    });
}

fn ha_card(ui: &mut Ui, status: &PoolStatus, actions: &mut Vec<UiAction>, read_only: bool) {
    theme::card_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            widgets::section_title(ui, "High availability", None);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if status.ha.enabled {
                    if ui
                        .add_enabled(
                            !read_only,
                            egui::Button::new(RichText::new("Disable HA…").size(10.5)),
                        )
                        .clicked()
                    {
                        actions.push(UiAction::PromptHaDisable);
                    }
                } else if ui
                    .add_enabled(
                        !read_only,
                        egui::Button::new(RichText::new("Enable HA…").size(10.5)),
                    )
                    .clicked()
                {
                    actions.push(UiAction::OpenHaEnable);
                }
            });
        });
        ui.add_space(4.0);
        widgets::kv(
            ui,
            "Enabled",
            if status.ha.enabled { "Yes" } else { "No" },
            false,
        );
        widgets::kv(
            ui,
            "Host failures tolerated",
            &status.ha.failures_to_tolerate.to_string(),
            false,
        );
        widgets::kv(
            ui,
            "Plan exists for",
            &format!("{} host(s)", status.ha.plan_exists_for),
            false,
        );
        widgets::kv(
            ui,
            "Cluster stack",
            if status.ha.cluster_stack.is_empty() {
                "—"
            } else {
                &status.ha.cluster_stack
            },
            false,
        );
        widgets::kv(
            ui,
            "Overcommitted",
            if status.ha.overcommitted { "Yes" } else { "No" },
            false,
        );
        widgets::kv(
            ui,
            "Allow overcommit",
            if status.ha.allow_overcommit {
                "Yes"
            } else {
                "No"
            },
            false,
        );
        widgets::kv(
            ui,
            "Redo log",
            if status.ha.redo_log_enabled {
                "Enabled"
            } else {
                "Disabled"
            },
            false,
        );
        widgets::kv(
            ui,
            "Redo log VDI",
            if status.ha.redo_log_vdi.is_empty() {
                "—"
            } else {
                &status.ha.redo_log_vdi
            },
            false,
        );
        widgets::kv(
            ui,
            "Reboot VMs on internal shutdown",
            if status.ha.reboot_vm_on_internal_shutdown {
                "Yes"
            } else {
                "No"
            },
            false,
        );
        if status.ha.statefiles.is_empty() {
            ui.label(RichText::new("No HA statefiles.").size(9.5).color(dim_fg()));
        } else {
            ui.label(
                RichText::new(format!(
                    "HA statefiles: {}",
                    status.ha.statefiles.join(", ")
                ))
                .size(9.5)
                .color(muted_fg()),
            );
        }
        if !status.ha.configuration.is_empty() {
            ui.add_space(4.0);
            config_table(ui, "ha-config", &status.ha.configuration);
        }
    });
}

fn wlb_card(ui: &mut Ui, status: &PoolStatus, actions: &mut Vec<UiAction>, read_only: bool) {
    theme::card_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            widgets::section_title(ui, "Workload balancing", None);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_enabled_ui(!read_only, |ui| {
                    ui.menu_button(RichText::new("Manage WLB…").size(10.5), |ui| {
                        if ui
                            .button(RichText::new("Configure WLB server…").size(10.5))
                            .clicked()
                        {
                            actions.push(UiAction::OpenWlbConfig);
                            ui.close();
                        }
                        if status.wlb.enabled {
                            if ui
                                .button(RichText::new("Disable WLB…").size(10.5))
                                .clicked()
                            {
                                actions.push(UiAction::PromptWlbToggle(false));
                                ui.close();
                            }
                        } else if ui.button(RichText::new("Enable WLB…").size(10.5)).clicked() {
                            actions.push(UiAction::PromptWlbToggle(true));
                            ui.close();
                        }
                        ui.separator();
                        if ui
                            .button(RichText::new("Deconfigure WLB…").size(10.5).color(amber()))
                            .clicked()
                        {
                            actions.push(UiAction::PromptWlbDeconfigure);
                            ui.close();
                        }
                    });
                });
            });
        });
        ui.add_space(4.0);
        widgets::kv(
            ui,
            "Enabled",
            if status.wlb.enabled { "Yes" } else { "No" },
            false,
        );
        widgets::kv(
            ui,
            "URL",
            if status.wlb.url.is_empty() {
                "—"
            } else {
                &status.wlb.url
            },
            false,
        );
        widgets::kv(
            ui,
            "Username",
            if status.wlb.username.is_empty() {
                "—"
            } else {
                &status.wlb.username
            },
            false,
        );
        widgets::kv(
            ui,
            "Verify certificate",
            if status.wlb.verify_cert { "Yes" } else { "No" },
            false,
        );
        if status.wlb.recommendations.is_empty() {
            ui.label(
                RichText::new("No WLB recommendations reported.")
                    .size(9.5)
                    .color(dim_fg()),
            );
        } else {
            config_table(ui, "wlb-recommendations", &status.wlb.recommendations);
        }
    });
}

fn pvs_card(ui: &mut Ui, status: &PoolStatus, actions: &mut Vec<UiAction>, read_only: bool) {
    let pvs = &status.pvs;
    theme::card_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            widgets::section_title(ui, "Provisioning services (PVS)", None);
            widgets::badge(ui, pvs.sites.len().to_string(), accent());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .add_enabled(
                        !read_only,
                        egui::Button::new(RichText::new("Add PVS site…").size(10.0)),
                    )
                    .clicked()
                {
                    actions.push(UiAction::OpenPvsSiteDialog);
                }
            });
        });
        ui.add_space(4.0);
        if pvs.sites.is_empty()
            && pvs.servers.is_empty()
            && pvs.proxies.is_empty()
            && pvs.cache_storages.is_empty()
        {
            ui.label(
                RichText::new("PVS is not configured on this pool.")
                    .size(10.0)
                    .color(dim_fg()),
            );
            return;
        }
        for site in &pvs.sites {
            ui.horizontal(|ui| {
                widgets::dot(ui, accent(), 6.0);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.label(
                        RichText::new(if site.name.is_empty() {
                            "(unnamed site)"
                        } else {
                            &site.name
                        })
                        .size(11.0)
                        .strong(),
                    );
                    ui.label(
                        RichText::new(format!(
                            "{} servers · {} proxies · {} cache stores",
                            site.servers, site.proxies, site.cache_storages
                        ))
                        .size(9.0)
                        .color(muted_fg()),
                    );
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .add_enabled(
                            !read_only,
                            egui::Button::new(RichText::new("Delete site…").size(9.5).color(red()))
                                .small(),
                        )
                        .clicked()
                    {
                        actions.push(UiAction::PromptPvsDelete {
                            kind: crate::state::PvsDeleteKind::Site,
                            uuid: site.uuid.clone(),
                            name: site.name.clone(),
                        });
                    }
                    if ui
                        .add_enabled(
                            !read_only,
                            egui::Button::new(RichText::new("Proxy…").size(9.5)).small(),
                        )
                        .clicked()
                    {
                        actions.push(UiAction::OpenPvsProxyDialog);
                    }
                    if ui
                        .add_enabled(
                            !read_only,
                            egui::Button::new(RichText::new("Cache…").size(9.5)).small(),
                        )
                        .clicked()
                    {
                        actions.push(UiAction::OpenPvsCacheDialog);
                    }
                    if ui
                        .add_enabled(
                            !read_only,
                            egui::Button::new(RichText::new("Server…").size(9.5)).small(),
                        )
                        .clicked()
                    {
                        actions.push(UiAction::OpenPvsServerDialog);
                    }
                });
            });
        }
        if !pvs.servers.is_empty() {
            ui.add_space(4.0);
            for server in &pvs.servers {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!(
                            "Server {} · {} · ports {}",
                            server.site_name, server.addresses, server.port_range
                        ))
                        .size(9.5)
                        .color(muted_fg()),
                    );
                    if ui
                        .add_enabled(
                            !read_only,
                            egui::Button::new(RichText::new("Remove").size(9.0)).small(),
                        )
                        .clicked()
                    {
                        actions.push(UiAction::PromptPvsDelete {
                            kind: crate::state::PvsDeleteKind::Server,
                            uuid: server.uuid.clone(),
                            name: server.addresses.clone(),
                        });
                    }
                });
            }
        }
        if !pvs.proxies.is_empty() {
            for proxy in &pvs.proxies {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!(
                            "Proxy {} · {} · attached {} · {}",
                            proxy.site_name, proxy.vif, proxy.attached, proxy.status
                        ))
                        .size(9.5)
                        .color(muted_fg()),
                    );
                    if ui
                        .add_enabled(
                            !read_only,
                            egui::Button::new(RichText::new("Remove").size(9.0)).small(),
                        )
                        .clicked()
                    {
                        actions.push(UiAction::PromptPvsDelete {
                            kind: crate::state::PvsDeleteKind::Proxy,
                            uuid: proxy.uuid.clone(),
                            name: proxy.site_name.clone(),
                        });
                    }
                });
            }
        }
        if !pvs.cache_storages.is_empty() {
            for cache in &pvs.cache_storages {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!(
                            "Cache {} · {} · {} · {}",
                            cache.site_name,
                            cache.host_name,
                            cache.sr_name,
                            crate::format::bytes(cache.size)
                        ))
                        .size(9.5)
                        .color(muted_fg()),
                    );
                    if ui
                        .add_enabled(
                            !read_only,
                            egui::Button::new(RichText::new("Remove").size(9.0)).small(),
                        )
                        .clicked()
                    {
                        actions.push(UiAction::PromptPvsDelete {
                            kind: crate::state::PvsDeleteKind::Cache,
                            uuid: cache.uuid.clone(),
                            name: cache.host_name.clone(),
                        });
                    }
                });
            }
        }
    });
}

fn vmss_card(ui: &mut Ui, status: &PoolStatus, actions: &mut Vec<UiAction>, read_only: bool) {
    theme::card_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            widgets::section_title(ui, "VM snapshot schedules", None);
            widgets::badge(ui, status.vmss.len().to_string(), purple());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .add_enabled(
                        !read_only,
                        egui::Button::new(RichText::new("New schedule…").size(10.0)),
                    )
                    .clicked()
                {
                    actions.push(UiAction::OpenVmssCreate);
                }
            });
        });
        ui.add_space(4.0);
        if status.vmss.is_empty() {
            ui.label(
                RichText::new("No VM snapshot schedules are configured.")
                    .size(10.0)
                    .color(dim_fg()),
            );
            return;
        }
        TableBuilder::new(ui)
            .id_salt("pool-vmss")
            .striped(true)
            .cell_layout(Layout::left_to_right(Align::Center))
            .column(Column::initial(160.0))
            .column(Column::exact(70.0))
            .column(Column::remainder().at_least(220.0))
            .column(Column::exact(80.0))
            .column(Column::exact(120.0))
            .header(26.0, |mut header| {
                for title in ["Name", "Enabled", "Schedule", "Retained", "Actions"] {
                    header.col(|ui| {
                        ui.label(RichText::new(title).size(9.5).color(muted_fg()));
                    });
                }
            })
            .body(|body| {
                body.rows(30.0, status.vmss.len(), |mut row| {
                    let schedule = &status.vmss[row.index()];
                    row.col(|ui| {
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 0.0;
                            ui.label(RichText::new(&schedule.name).size(10.5).strong());
                            ui.label(
                                RichText::new(format!(
                                    "{} · {}",
                                    schedule.schedule_type, schedule.frequency
                                ))
                                .size(8.5)
                                .color(dim_fg()),
                            );
                        });
                    });
                    row.col(|ui| {
                        ui.label(
                            RichText::new(if schedule.enabled { "Yes" } else { "No" })
                                .size(9.5)
                                .color(if schedule.enabled { green() } else { dim_fg() }),
                        );
                    });
                    row.col(|ui| {
                        ui.add(
                            egui::Label::new(
                                RichText::new(if schedule.schedule.is_empty() {
                                    "—"
                                } else {
                                    &schedule.schedule
                                })
                                .size(9.0)
                                .color(muted_fg()),
                            )
                            .truncate(),
                        );
                    });
                    row.col(|ui| {
                        ui.label(
                            RichText::new(schedule.retained_snapshots.to_string())
                                .monospace()
                                .size(9.5),
                        );
                    });
                    row.col(|ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;
                            if ui
                                .add_enabled(
                                    !read_only,
                                    egui::Button::new(RichText::new("Edit").size(9.5)).small(),
                                )
                                .clicked()
                            {
                                actions.push(UiAction::OpenVmssEdit(schedule.uuid.clone()));
                            }
                            if ui
                                .add_enabled(
                                    !read_only,
                                    egui::Button::new(
                                        RichText::new("Delete").size(9.5).color(red()),
                                    )
                                    .small(),
                                )
                                .clicked()
                            {
                                actions.push(UiAction::PromptVmssDelete {
                                    uuid: schedule.uuid.clone(),
                                    name: schedule.name.clone(),
                                });
                            }
                        });
                    });
                });
            });
    });
}

fn dr_card(ui: &mut Ui, status: &PoolStatus, actions: &mut Vec<UiAction>, read_only: bool) {
    theme::card_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            widgets::section_title(ui, "Disaster recovery tasks", None);
            widgets::badge(ui, status.dr.len().to_string(), cyan());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .add_enabled(
                        !read_only,
                        egui::Button::new(RichText::new("Create DR task…").size(10.0)),
                    )
                    .clicked()
                {
                    actions.push(UiAction::OpenDrTaskDialog);
                }
            });
        });
        ui.add_space(4.0);
        if status.dr.is_empty() {
            ui.label(
                RichText::new("No DR tasks are configured.")
                    .size(10.0)
                    .color(dim_fg()),
            );
            return;
        }
        for task in &status.dr {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!(
                        "{} · introduced SRs: {}",
                        task.uuid,
                        if task.introduced_srs.is_empty() {
                            "—".to_owned()
                        } else {
                            task.introduced_srs.join(", ")
                        }
                    ))
                    .monospace()
                    .size(9.0)
                    .color(muted_fg()),
                );
                if ui
                    .add_enabled(
                        !read_only,
                        egui::Button::new(RichText::new("Destroy…").size(9.0).color(red())).small(),
                    )
                    .clicked()
                {
                    actions.push(UiAction::PromptDrTaskDestroy {
                        uuid: task.uuid.clone(),
                        introduced_srs: task.introduced_srs.clone(),
                    });
                }
            });
        }
    });
}

fn sriov_card(ui: &mut Ui, status: &PoolStatus) {
    let sriov = &status.sriov;
    theme::card_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            widgets::section_title(ui, "SR-IOV, tunnels and internal networks", None);
            widgets::badge(ui, format!("{} PIF", sriov.pifs.len()), accent());
            widgets::badge(ui, format!("{} tunnel", sriov.tunnels.len()), cyan());
            widgets::badge(
                ui,
                format!("{} network_sriov", sriov.network_sriovs.len()),
                purple(),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                widgets::badge(ui, "Read-only discovery", muted_fg());
            });
        });
        ui.add_space(4.0);
        if sriov.pifs.is_empty() && sriov.tunnels.is_empty() && sriov.network_sriovs.is_empty() {
            ui.label(
                RichText::new(
                    "No SR-IOV or tunnel objects are configured. This view is ready for SR-IOV NICs and cross-pool tunnels when the pool has them.",
                )
                .size(10.0)
                .color(dim_fg()),
            );
        }
        if !sriov.pifs.is_empty() {
            ui.add_space(4.0);
            for pif in &sriov.pifs {
                ui.label(
                    RichText::new(format!(
                        "{} · {} · physical of {} · logical of {}",
                        pif.host_name,
                        pif.device,
                        pif.physical_of.len(),
                        pif.logical_of.len()
                    ))
                    .size(9.5)
                    .color(muted_fg()),
                );
            }
        }
        if !sriov.network_sriovs.is_empty() {
            for entry in &sriov.network_sriovs {
                ui.label(
                    RichText::new(format!(
                        "network_sriov {} · {} → {} · reboot={} · {}",
                        entry.uuid,
                        entry.physical_device,
                        entry.logical_device,
                        entry.requires_reboot,
                        entry.configuration_mode
                    ))
                    .monospace()
                    .size(9.0)
                    .color(muted_fg()),
                );
            }
        }
        if !sriov.tunnels.is_empty() {
            for tunnel in &sriov.tunnels {
                ui.label(
                    RichText::new(format!(
                        "tunnel {} · {} · {} → {} · {}",
                        tunnel.uuid,
                        tunnel.protocol,
                        tunnel.access_device,
                        tunnel.transport_device,
                        tunnel.status
                    ))
                    .monospace()
                    .size(9.0)
                    .color(muted_fg()),
                );
            }
        }
        if sriov.networks.is_empty() {
            return;
        }
        ui.add_space(6.0);
        TableBuilder::new(ui)
            .id_salt("pool-sriov-networks")
            .striped(true)
            .cell_layout(Layout::left_to_right(Align::Center))
            .column(Column::remainder().at_least(180.0))
            .column(Column::initial(170.0))
            .column(Column::exact(70.0))
            .column(Column::initial(110.0))
            .column(Column::exact(150.0))
            .header(24.0, |mut header| {
                for title in ["Network", "Purpose", "MTU", "Bridge", "Kind"] {
                    header.col(|ui| {
                        ui.label(RichText::new(title).size(9.0).color(muted_fg()));
                    });
                }
            })
            .body(|body| {
                body.rows(26.0, sriov.networks.len(), |mut row| {
                    let network = &sriov.networks[row.index()];
                    row.col(|ui| {
                        ui.add(
                            egui::Label::new(RichText::new(&network.name).size(10.0)).truncate(),
                        );
                    });
                    row.col(|ui| {
                        ui.add(
                            egui::Label::new(
                                RichText::new(if network.purpose.is_empty() {
                                    "—".to_owned()
                                } else {
                                    network.purpose.join(", ")
                                })
                                .size(9.0)
                                .color(muted_fg()),
                            )
                            .truncate(),
                        );
                    });
                    row.col(|ui| {
                        ui.label(
                            RichText::new(network.mtu.to_string())
                                .monospace()
                                .size(9.0),
                        );
                    });
                    row.col(|ui| {
                        ui.add(
                            egui::Label::new(
                                RichText::new(if network.bridge.is_empty() {
                                    "—"
                                } else {
                                    &network.bridge
                                })
                                .monospace()
                                .size(9.0)
                                .color(muted_fg()),
                            )
                            .truncate(),
                        );
                    });
                    row.col(|ui| {
                        let (label, color) = if network.host_internal {
                            ("Host internal", amber())
                        } else if network.guest_installer {
                            ("Guest installer", purple())
                        } else if network
                            .purpose
                            .iter()
                            .any(|purpose| purpose.contains("sriov"))
                        {
                            ("SR-IOV", accent())
                        } else {
                            ("Regular", dim_fg())
                        };
                        ui.label(RichText::new(label).size(9.0).color(color));
                    });
                });
            });
    });
}

fn config_table(ui: &mut Ui, id: &str, entries: &[PoolConfigEntry]) {
    TableBuilder::new(ui)
        .id_salt(id)
        .striped(true)
        .cell_layout(Layout::left_to_right(Align::Center))
        .column(Column::remainder().at_least(120.0))
        .column(Column::remainder().at_least(140.0))
        .header(24.0, |mut header| {
            for title in ["Setting", "Value"] {
                header.col(|ui| {
                    ui.label(RichText::new(title).size(9.0).color(muted_fg()));
                });
            }
        })
        .body(|body| {
            body.rows(24.0, entries.len(), |mut row| {
                let entry = &entries[row.index()];
                row.col(|ui| {
                    ui.label(RichText::new(&entry.key).monospace().size(9.0));
                });
                row.col(|ui| {
                    ui.label(
                        RichText::new(&entry.value)
                            .monospace()
                            .size(9.0)
                            .color(if entry.sensitive { amber() } else { fg() }),
                    );
                });
            });
        });
}
