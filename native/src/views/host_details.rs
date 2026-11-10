use egui::{
    Align, Color32, CornerRadius, Label, Layout, Margin, RichText, Sense, Stroke, Ui, Vec2,
};
use egui_extras::{Column, TableBuilder};
use xen_core::{
    host::{DataSection, DetailRecord},
    operations::HostAction,
    performance::{HostPerformance, MetricSeries},
    workspace::WorkspaceView,
};

use crate::format;
use crate::state::{
    HostTab, LoadState, RecordDialog, UiAction, UiState, is_read_only, section_columns,
};
use crate::theme;
use crate::theme::*;
use crate::widgets;

const RECORD_PAGE: usize = 25;
const METRIC_PAGE: usize = 12;

pub fn show(
    ui: &mut Ui,
    state: &mut UiState,
    workspace: &WorkspaceView,
    actions: &mut Vec<UiAction>,
) {
    let Some(route) = state.host_route.clone() else {
        return;
    };
    let read_only =
        crate::state::find_connection(workspace, &route.connection_id).is_some_and(is_read_only);

    ui.horizontal(|ui| {
        if ui
            .add(egui::Button::new(RichText::new("‹  Back to hosts").size(10.5)).frame(false))
            .clicked()
        {
            actions.push(UiAction::CloseHostDetails);
        }
        ui.label(RichText::new("·").size(10.0).color(dim_fg()));
        ui.label(
            RichText::new(
                crate::state::find_connection(workspace, &route.connection_id).map_or_else(
                    || route.connection_id.clone(),
                    crate::state::connection_name,
                ),
            )
            .size(10.0)
            .color(muted_fg()),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui
                .add_enabled(
                    !state.host_state.is_loading(),
                    egui::Button::new(RichText::new("Refresh details").size(10.5)),
                )
                .clicked()
            {
                actions.push(UiAction::ReloadHostDetails);
            }
            if let Some(details) = state.host_state.ready()
                && !read_only
            {
                ui.menu_button(RichText::new("Host actions…").size(10.5), |ui| {
                    let host_uuid = route.host_uuid.clone();
                    let host_name = details.name.clone();
                    let mut prompt = |ui: &mut Ui, action: HostAction| {
                        let color = if action.destructive() { red() } else { fg() };
                        if ui
                            .button(RichText::new(action.title()).size(11.0).color(color))
                            .clicked()
                        {
                            actions.push(UiAction::PromptHostAction {
                                host_uuid: host_uuid.clone(),
                                host_name: host_name.clone(),
                                action,
                            });
                            ui.close();
                        }
                    };
                    prompt(
                        ui,
                        if details.enabled {
                            HostAction::Disable
                        } else {
                            HostAction::Enable
                        },
                    );
                    prompt(ui, HostAction::Evacuate);
                    prompt(ui, HostAction::RestartToolstack);
                    ui.separator();
                    prompt(ui, HostAction::Reboot);
                    prompt(ui, HostAction::Shutdown);
                });
            }
            widgets::badge(
                ui,
                if read_only {
                    "Read-only connection"
                } else {
                    "Read-only host view"
                },
                amber(),
            );
        });
    });
    ui.add_space(6.0);

    let ready = state.host_state.ready();
    let name = ready.map_or(route.host_name.clone(), |details| details.name.clone());
    let address = ready.map_or_else(String::new, |details| details.address.clone());
    ui.horizontal(|ui| {
        widgets::server_mark(ui, 42.0);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.label(RichText::new("HOST DETAILS").size(8.5).color(dim_fg()));
            ui.label(RichText::new(name).size(23.0).strong());
            ui.horizontal(|ui| {
                if !address.is_empty() {
                    ui.label(RichText::new(address).size(10.0).color(muted_fg()));
                    ui.label(RichText::new("·").size(10.0).color(dim_fg()));
                }
                ui.label(
                    RichText::new(&route.host_uuid)
                        .monospace()
                        .size(9.0)
                        .color(dim_fg()),
                );
            });
        });
    });
    ui.add_space(8.0);

    match &state.host_state {
        LoadState::Idle | LoadState::Loading => {
            ui.add_space(20.0);
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(
                    RichText::new("Loading host details…")
                        .size(11.0)
                        .color(muted_fg()),
                );
            });
            return;
        }
        LoadState::Error(message) => {
            let message = message.clone();
            ui.add_space(10.0);
            theme::card_frame().show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new("Could not load host information").strong());
                ui.label(RichText::new(&message).size(11.0).color(red()));
                if ui.button("Retry").clicked() {
                    actions.push(UiAction::ReloadHostDetails);
                }
            });
            return;
        }
        LoadState::Ready(_) => {}
    }

    let Some(details) = state.host_state.ready().cloned() else {
        return;
    };

    if !details.warnings.is_empty() {
        egui::CollapsingHeader::new(
            RichText::new(format!(
                "{} collection notices · some host information may be incomplete",
                details.warnings.len()
            ))
            .size(10.0)
            .color(amber()),
        )
        .default_open(false)
        .show(ui, |ui| {
            for warning in &details.warnings {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(&warning.section)
                            .size(9.5)
                            .strong()
                            .color(amber()),
                    );
                    ui.add(
                        Label::new(RichText::new(&warning.message).size(9.5).color(muted_fg()))
                            .wrap(),
                    );
                });
            }
        });
    }

    let tabs: Vec<&str> = HostTab::ALL.iter().map(|tab| tab.label()).collect();
    let mut index = HostTab::ALL
        .iter()
        .position(|tab| *tab == state.host_tab)
        .unwrap_or(0);
    widgets::tab_bar(ui, &tabs, &mut index);
    ui.add_space(6.0);
    if HostTab::ALL[index] != state.host_tab {
        actions.push(UiAction::SetHostTab(HostTab::ALL[index]));
    }

    match HostTab::ALL[index] {
        HostTab::General => widgets::info_groups(ui, &details.general, 2),
        HostTab::Cpu => {
            ui.label(
                RichText::new(
                    "Processor topology, host memory and available hardware information.",
                )
                .size(10.5)
                .color(muted_fg()),
            );
            ui.add_space(6.0);
            widgets::info_groups(ui, &details.memory, 2);
            record_section(
                ui,
                state,
                &details.cpu,
                "Physical CPUs",
                Some("Logical processors reported by host_cpu."),
                actions,
            );
        }
        HostTab::Performance => performance(ui, state, actions),
        HostTab::Networking => record_section(
            ui,
            state,
            &details.nics,
            "Network interfaces",
            Some("Physical NICs, bonds, VLANs and management networking reported by the host."),
            actions,
        ),
        HostTab::Storage => record_section(
            ui,
            state,
            &details.storage,
            "Storage repositories",
            Some("Host storage attachment and repository capacity."),
            actions,
        ),
        HostTab::Hardware => {
            record_section(ui, state, &details.gpus, "GPUs", None, actions);
            record_section(ui, state, &details.pci, "PCI devices", None, actions);
            record_section(ui, state, &details.usb, "USB devices", None, actions);
        }
        HostTab::Updates => record_section(
            ui,
            state,
            &details.updates,
            "Updates & patches",
            Some("Installed and available update information exposed by the host API."),
            actions,
        ),
        HostTab::Certificates => record_section(
            ui,
            state,
            &details.certificates,
            "Certificates",
            None,
            actions,
        ),
        HostTab::Health => {
            record_section(ui, state, &details.alerts, "Alerts", None, actions);
            record_section(ui, state, &details.crashdumps, "Crash dumps", None, actions);
        }
    }
}

fn record_section(
    ui: &mut Ui,
    state: &mut UiState,
    section: &DataSection,
    title: &str,
    subtitle: Option<&str>,
    _actions: &mut Vec<UiAction>,
) {
    let (status, message, rows) = match section {
        DataSection::Ready { rows, .. } => ("ready", String::new(), rows.clone()),
        DataSection::Unsupported { message } => ("unsupported", message.clone(), Vec::new()),
        DataSection::Unavailable { message } => ("unavailable", message.clone(), Vec::new()),
    };
    let columns = section_columns(section);

    theme::card_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new(title).size(13.0).strong());
                if let Some(subtitle) = subtitle {
                    ui.label(RichText::new(subtitle).size(9.5).color(muted_fg()));
                }
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if status == "ready" {
                    widgets::badge(
                        ui,
                        format!(
                            "{} record{}",
                            rows.len(),
                            if rows.len() == 1 { "" } else { "s" }
                        ),
                        muted_fg(),
                    );
                }
            });
        });
        ui.add_space(4.0);

        if status != "ready" {
            theme::inset_frame().show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(
                    RichText::new(if status == "unsupported" {
                        "Not supported by this host"
                    } else {
                        "Information unavailable"
                    })
                    .size(11.0)
                    .strong()
                    .color(if status == "unsupported" {
                        muted_fg()
                    } else {
                        red()
                    }),
                );
                ui.label(RichText::new(&message).size(10.0).color(muted_fg()));
            });
            ui.add_space(GAP);
            return;
        }

        if rows.is_empty() {
            ui.label(
                RichText::new(format!("No {} reported by the host.", title.to_lowercase()))
                    .size(10.5)
                    .color(dim_fg()),
            );
            ui.add_space(GAP);
            return;
        }

        ui.horizontal(|ui| {
            ui.scope(|ui| {
                ui.set_width(260.0);
                widgets::search_field(
                    ui,
                    &mut state.record_query,
                    &format!("Search {}…", title.to_lowercase()),
                );
            });
        });
        ui.add_space(4.0);

        let needle = state.record_query.trim().to_lowercase();
        let filtered: Vec<DetailRecord> = rows
            .into_iter()
            .filter(|record| {
                needle.is_empty()
                    || record.title.to_lowercase().contains(&needle)
                    || record.id.to_lowercase().contains(&needle)
                    || record
                        .values
                        .values()
                        .any(|value| value.to_lowercase().contains(&needle))
                    || record.groups.iter().any(|group| {
                        group.title.to_lowercase().contains(&needle)
                            || group.fields.iter().any(|field| {
                                field.label.to_lowercase().contains(&needle)
                                    || field.value.to_lowercase().contains(&needle)
                            })
                    })
            })
            .collect();
        let total = filtered.len();
        let page_count = total.div_ceil(RECORD_PAGE).max(1);
        if state.record_page >= page_count {
            state.record_page = page_count - 1;
        }
        let page = state.record_page;
        let page_rows: Vec<DetailRecord> = filtered
            .into_iter()
            .skip(page * RECORD_PAGE)
            .take(RECORD_PAGE)
            .collect();

        let mut builder = TableBuilder::new(ui)
            .id_salt(("record-table", title))
            .striped(true)
            .cell_layout(Layout::left_to_right(Align::Center))
            .vscroll(false);
        for (index, _) in columns.iter().enumerate() {
            builder = if index == 0 {
                builder.column(Column::remainder().at_least(120.0))
            } else {
                builder.column(Column::initial(125.0).at_least(70.0))
            };
        }
        builder
            .header(24.0, |mut header| {
                for column in &columns {
                    header.col(|ui| {
                        ui.label(RichText::new(column).size(9.0).color(muted_fg()));
                    });
                }
            })
            .body(|body| {
                body.rows(28.0, page_rows.len(), |mut row| {
                    let record = &page_rows[row.index()];
                    for (index, column) in columns.iter().enumerate() {
                        row.col(|ui| {
                            let value = record.values.get(column).cloned().unwrap_or_default();
                            if index == 0 {
                                if ui
                                    .add(
                                        Label::new(RichText::new(value).size(10.5))
                                            .truncate()
                                            .sense(Sense::click()),
                                    )
                                    .clicked()
                                {
                                    state.record_dialog = Some(record_dialog(record, title));
                                }
                            } else {
                                ui.add(
                                    Label::new(
                                        RichText::new(if value.is_empty() {
                                            "—".to_owned()
                                        } else {
                                            value
                                        })
                                        .size(10.0),
                                    )
                                    .truncate(),
                                );
                            }
                        });
                    }
                });
            });

        if total > RECORD_PAGE {
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!(
                        "{}–{} of {total}",
                        page * RECORD_PAGE + 1,
                        ((page + 1) * RECORD_PAGE).min(total)
                    ))
                    .size(9.0)
                    .color(dim_fg()),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .add_enabled(
                            page + 1 < page_count,
                            egui::Button::new(RichText::new("Next").size(9.5)),
                        )
                        .clicked()
                    {
                        state.record_page += 1;
                    }
                    if ui
                        .add_enabled(
                            page > 0,
                            egui::Button::new(RichText::new("Previous").size(9.5)),
                        )
                        .clicked()
                    {
                        state.record_page -= 1;
                    }
                });
            });
        }
    });
    ui.add_space(GAP);
}

fn record_dialog(record: &DetailRecord, section: &str) -> RecordDialog {
    RecordDialog {
        title: record.title.clone(),
        subtitle: format!("{section} · {}", record.id),
        values: record
            .values
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect(),
        groups: record.groups.clone(),
    }
}

fn performance(ui: &mut Ui, state: &mut UiState, actions: &mut Vec<UiAction>) {
    theme::card_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new("Host performance").size(13.0).strong());
                ui.label(
                    RichText::new(
                        "Host RRD samples · refresh manually · gaps indicate missing data",
                    )
                    .size(9.5)
                    .color(muted_fg()),
                );
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .add_enabled(
                        !state.perf_state.is_loading(),
                        egui::Button::new(RichText::new("Refresh").size(10.0)),
                    )
                    .clicked()
                {
                    actions.push(UiAction::LoadPerformance);
                }
                for (seconds, label) in [(86400u64, "24 hours"), (3600, "1 hour"), (900, "15 min")]
                {
                    let selected = state.perf_range == seconds;
                    if ui
                        .add(
                            egui::Button::new(RichText::new(label).size(10.0).color(if selected {
                                fg()
                            } else {
                                muted_fg()
                            }))
                            .fill(if selected {
                                panel_3()
                            } else {
                                Color32::TRANSPARENT
                            })
                            .frame(selected),
                        )
                        .clicked()
                        && !selected
                    {
                        state.perf_range = seconds;
                        actions.push(UiAction::LoadPerformance);
                    }
                }
            });
        });
        ui.add_space(8.0);

        let perf = state.perf_state.clone();
        match perf {
            LoadState::Idle => {
                ui.label(
                    RichText::new("Select a range to load host metrics.")
                        .size(10.5)
                        .color(muted_fg()),
                );
            }
            LoadState::Loading => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(
                        RichText::new("Loading performance samples…")
                            .size(10.5)
                            .color(muted_fg()),
                    );
                });
            }
            LoadState::Error(message) => {
                let message = message.clone();
                ui.label(RichText::new(&message).size(10.5).color(red()));
                if ui.button("Retry").clicked() {
                    actions.push(UiAction::LoadPerformance);
                }
            }
            LoadState::Ready(performance) => match performance {
                HostPerformance::Unsupported { message }
                | HostPerformance::Unavailable { message } => {
                    ui.label(RichText::new(message).size(10.5).color(muted_fg()));
                }
                HostPerformance::Ready {
                    start,
                    end,
                    step,
                    series,
                } => metrics(ui, state, start, end, step, &series),
            },
        }
    });
}

fn metrics(
    ui: &mut Ui,
    state: &mut UiState,
    start: u64,
    end: u64,
    step: u64,
    series: &[MetricSeries],
) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!(
                "{} — {}",
                format::timestamp(start),
                format::timestamp(end)
            ))
            .size(10.0)
            .color(muted_fg()),
        );
        ui.label(RichText::new("·").size(10.0).color(dim_fg()));
        ui.label(
            RichText::new(format!(
                "{step}s sample interval · {} metrics",
                series.len()
            ))
            .size(10.0)
            .color(muted_fg()),
        );
    });
    if series.is_empty() {
        widgets::empty_state(
            ui,
            "No performance metrics reported",
            "The host returned no series for this time range.",
        );
        return;
    }
    ui.add_space(4.0);
    ui.scope(|ui| {
        ui.set_width(300.0);
        widgets::search_field(ui, &mut state.perf_query, "Search performance metrics…");
    });
    let needle = state.perf_query.trim().to_lowercase();
    let matches = |metric: &MetricSeries| {
        needle.is_empty()
            || metric.name.to_lowercase().contains(&needle)
            || pretty_metric_name(&metric.name)
                .to_lowercase()
                .contains(&needle)
    };

    let cpu: Vec<&MetricSeries> = series
        .iter()
        .filter(|metric| is_cpu_core(&metric.name) && matches(metric))
        .collect();
    let memory: Vec<&MetricSeries> = series
        .iter()
        .filter(|metric| is_memory_metric(&metric.name) && matches(metric))
        .collect();
    let network: Vec<&MetricSeries> = series
        .iter()
        .filter(|metric| is_network_metric(&metric.name) && matches(metric))
        .collect();
    let used: std::collections::HashSet<&str> = cpu
        .iter()
        .chain(&memory)
        .chain(&network)
        .map(|metric| metric.name.as_str())
        .collect();
    let other: Vec<&MetricSeries> = series
        .iter()
        .filter(|metric| !used.contains(metric.name.as_str()) && matches(metric))
        .collect();

    let mut graph_count = 0;
    for (title, group) in [
        ("CPU Performance", cpu),
        ("Memory Performance", memory),
        ("Network Performance", network),
    ] {
        if group.is_empty() {
            continue;
        }
        multi_chart(ui, title, &group, start, end, step);
        ui.add_space(10.0);
        graph_count += 1;
    }

    if !other.is_empty() {
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Other host metrics").size(14.0).strong());
            widgets::badge(ui, other.len().to_string(), muted_fg());
        });
        ui.label(
            RichText::new("One card per remaining data source.")
                .size(10.0)
                .color(dim_fg()),
        );
        ui.add_space(6.0);
        let total = other.len();
        let page_count = total.div_ceil(METRIC_PAGE).max(1);
        if state.perf_page >= page_count {
            state.perf_page = page_count - 1;
        }
        let page = state.perf_page;
        let page_series: Vec<&MetricSeries> = other
            .iter()
            .skip(page * METRIC_PAGE)
            .take(METRIC_PAGE)
            .copied()
            .collect();
        let gap = 10.0;
        let chart_width = ((ui.available_width() - gap) / 2.0).max(220.0);
        for chunk in page_series.chunks(2) {
            ui.scope(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.horizontal_top(|ui| {
                    ui.allocate_ui_with_layout(
                        Vec2::new(chart_width, 0.0),
                        Layout::top_down(Align::LEFT),
                        |ui| {
                            ui.set_width(chart_width);
                            chart(ui, chunk[0], start, end, step);
                        },
                    );
                    if chunk.len() > 1 {
                        ui.add_space(gap);
                        ui.allocate_ui_with_layout(
                            Vec2::new(chart_width, 0.0),
                            Layout::top_down(Align::LEFT),
                            |ui| {
                                ui.set_width(chart_width);
                                chart(ui, chunk[1], start, end, step);
                            },
                        );
                    }
                });
            });
            ui.add_space(gap);
        }
        if total > METRIC_PAGE {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!(
                        "{}–{} of {total} metrics",
                        page * METRIC_PAGE + 1,
                        ((page + 1) * METRIC_PAGE).min(total)
                    ))
                    .size(9.5)
                    .color(dim_fg()),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .add_enabled(
                            page + 1 < page_count,
                            egui::Button::new(RichText::new("Next").size(10.0)),
                        )
                        .clicked()
                    {
                        state.perf_page += 1;
                    }
                    if ui
                        .add_enabled(
                            page > 0,
                            egui::Button::new(RichText::new("Previous").size(10.0)),
                        )
                        .clicked()
                    {
                        state.perf_page -= 1;
                    }
                });
            });
        }
    }

    if graph_count == 0 && other.is_empty() {
        widgets::empty_state(
            ui,
            "No metrics match the search",
            "Clear the search or select another range.",
        );
    }
}

pub(crate) fn is_cpu_core(name: &str) -> bool {
    name.strip_prefix("cpu").is_some_and(|rest| {
        !rest.is_empty() && rest.chars().all(|character| character.is_ascii_digit())
    })
}

pub(crate) fn is_memory_metric(name: &str) -> bool {
    matches!(
        name,
        "memory_total_kib" | "memory_free_kib" | "memory" | "memory_internal_free"
    )
}

pub(crate) fn is_network_metric(name: &str) -> bool {
    name.starts_with("pif_") || name.starts_with("vif_")
}

fn graph_color(index: usize) -> egui::Color32 {
    const COLORS: [egui::Color32; 10] = [
        egui::Color32::from_rgb(37, 99, 235),
        egui::Color32::from_rgb(16, 145, 100),
        egui::Color32::from_rgb(217, 119, 6),
        egui::Color32::from_rgb(124, 92, 214),
        egui::Color32::from_rgb(204, 60, 60),
        egui::Color32::from_rgb(14, 116, 144),
        egui::Color32::from_rgb(180, 83, 9),
        egui::Color32::from_rgb(22, 163, 74),
        egui::Color32::from_rgb(147, 51, 234),
        egui::Color32::from_rgb(100, 116, 139),
    ];
    COLORS[index % COLORS.len()]
}

fn metric_value_text(unit: &str, value: f64) -> String {
    if unit == "%" {
        format!("{value:.2} %")
    } else if unit == "B" {
        format::bytes(value.max(0.0) as u64)
    } else if unit.is_empty() {
        format_axis(value)
    } else {
        format!("{} {}", format_axis(value), unit)
    }
}

fn latest_value(series: &MetricSeries) -> Option<f64> {
    series.points.iter().rev().find_map(|point| point.value)
}

fn point_at(series: &MetricSeries, time: u64) -> Option<&xen_core::performance::MetricPoint> {
    series
        .points
        .iter()
        .filter(|point| point.value.is_some())
        .min_by_key(|point| point.timestamp.abs_diff(time))
}

pub(crate) fn multi_chart(
    ui: &mut Ui,
    title: &str,
    series: &[&MetricSeries],
    start: u64,
    end: u64,
    step: u64,
) {
    theme::card_frame()
        .inner_margin(Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new(title).size(15.0).strong());
                widgets::badge(ui, format!("{} series", series.len()), muted_fg());
            });
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                for (index, metric) in series.iter().enumerate() {
                    let color = graph_color(index);
                    let (swatch, _) = ui.allocate_exact_size(Vec2::new(14.0, 3.0), Sense::hover());
                    ui.painter()
                        .rect_filled(swatch, egui::CornerRadius::same(2), color);
                    let value = latest_value(metric)
                        .map(|value| metric_value_text(&metric.unit, value))
                        .unwrap_or_else(|| "No sample".into());
                    ui.label(
                        RichText::new(format!("{}  {value}", pretty_metric_name(&metric.name)))
                            .size(10.0)
                            .color(muted_fg()),
                    );
                }
            });
            ui.add_space(6.0);

            let width = ui.available_width().max(320.0);
            let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 210.0), Sense::hover());
            let painter = ui.painter();
            painter.rect_filled(rect, egui::CornerRadius::same(8), panel_2());
            painter.rect_stroke(
                rect,
                egui::CornerRadius::same(8),
                Stroke::new(1.0, border_soft()),
                egui::StrokeKind::Inside,
            );

            let values: Vec<f64> = series
                .iter()
                .flat_map(|metric| metric.points.iter().filter_map(|point| point.value))
                .collect();
            if values.is_empty() {
                painter.text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "No numeric samples available",
                    egui::FontId::proportional(10.5),
                    dim_fg(),
                );
                return;
            }
            let percent = series.first().is_some_and(|metric| metric.unit == "%");
            let min = values.iter().copied().fold(f64::INFINITY, f64::min);
            let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            let low = if percent {
                0.0
            } else {
                min - (max - min).max(1.0) * 0.12
            };
            let high = if percent {
                (max * 1.08).clamp(1.0, 100.0)
            } else {
                max + (max - min).max(1.0) * 0.12
            };
            let plot = egui::Rect::from_min_max(
                egui::pos2(rect.left() + 52.0, rect.top() + 12.0),
                egui::pos2(rect.right() - 10.0, rect.bottom() - 22.0),
            );
            let x = |time: u64| {
                plot.left()
                    + ((time.saturating_sub(start)) as f32
                        / (end.saturating_sub(start)).max(1) as f32)
                        * plot.width()
            };
            let y =
                |value: f64| plot.bottom() - ((value - low) / (high - low)) as f32 * plot.height();

            for fraction in [0.0, 0.5, 1.0] {
                let value = low + fraction * (high - low);
                let py = y(value);
                painter.hline(plot.x_range(), py, Stroke::new(1.0, border_soft()));
                painter.text(
                    egui::pos2(plot.left() - 6.0, py),
                    egui::Align2::RIGHT_CENTER,
                    if percent {
                        format!("{value:.0}%")
                    } else {
                        format_axis(value)
                    },
                    egui::FontId::monospace(9.0),
                    dim_fg(),
                );
            }

            for (index, metric) in series.iter().enumerate() {
                let color = graph_color(index);
                let mut segment = Vec::new();
                let mut previous_time = 0u64;
                for point in &metric.points {
                    let Some(value) = point.value else {
                        flush_colored(painter, &mut segment, color);
                        continue;
                    };
                    let contiguous = segment.is_empty()
                        || step == 0
                        || point.timestamp.saturating_sub(previous_time) <= step.saturating_mul(2);
                    if !contiguous {
                        flush_colored(painter, &mut segment, color);
                    }
                    segment.push(egui::pos2(x(point.timestamp), y(value)));
                    previous_time = point.timestamp;
                }
                flush_colored(painter, &mut segment, color);
            }

            painter.text(
                egui::pos2(plot.left(), rect.bottom() - 8.0),
                egui::Align2::LEFT_BOTTOM,
                format::clock(start),
                egui::FontId::monospace(9.0),
                dim_fg(),
            );
            painter.text(
                egui::pos2(plot.right(), rect.bottom() - 8.0),
                egui::Align2::RIGHT_BOTTOM,
                format::clock(end),
                egui::FontId::monospace(9.0),
                dim_fg(),
            );

            if let Some(pointer) = response.hover_pos()
                && plot.contains(pointer)
            {
                let fraction =
                    ((pointer.x - plot.left()) / plot.width().max(1.0)).clamp(0.0, 1.0) as f64;
                let target = start as f64 + fraction * (end.saturating_sub(start)) as f64;
                let nearest_time = series
                    .iter()
                    .flat_map(|metric| metric.points.iter().filter(|point| point.value.is_some()))
                    .min_by_key(|point| (point.timestamp as f64 - target).abs() as u64)
                    .map(|point| point.timestamp);
                if let Some(time) = nearest_time {
                    let px = x(time);
                    painter.vline(px, plot.y_range(), Stroke::new(1.0, with_alpha(fg(), 100)));
                    for (index, metric) in series.iter().enumerate() {
                        if let Some(point) = point_at(metric, time)
                            && let Some(value) = point.value
                        {
                            painter.circle_filled(
                                egui::pos2(px, y(value)),
                                2.8,
                                graph_color(index),
                            );
                        }
                    }
                    painter.text(
                        egui::pos2(plot.right() - 2.0, rect.top() + 6.0),
                        egui::Align2::RIGHT_TOP,
                        format::short_time(time),
                        egui::FontId::monospace(9.0),
                        fg(),
                    );
                }
            }
        });
}

fn flush_colored(painter: &egui::Painter, segment: &mut Vec<egui::Pos2>, color: egui::Color32) {
    if segment.len() > 1 {
        painter.add(egui::Shape::line(
            std::mem::take(segment),
            Stroke::new(1.5, color),
        ));
    } else {
        segment.clear();
    }
}

pub(crate) fn chart(ui: &mut Ui, series: &MetricSeries, start: u64, end: u64, step: u64) {
    let latest = series
        .points
        .iter()
        .rev()
        .find(|point| point.value.is_some());
    theme::card_frame()
        .inner_margin(Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (swatch, _) = ui.allocate_exact_size(Vec2::new(12.0, 3.0), Sense::hover());
                ui.painter()
                    .rect_filled(swatch, CornerRadius::same(2), cyan());
                ui.add_space(2.0);
                ui.label(
                    RichText::new(pretty_metric_name(&series.name))
                        .size(10.5)
                        .strong(),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if let Some(point) = latest
                        && let Some(value) = point.value
                    {
                        let text = if series.unit == "%" {
                            format!("{value:.2} %")
                        } else if series.unit == "B" {
                            format::bytes(value.max(0.0) as u64)
                        } else {
                            format!("{} {}", format_axis(value), series.unit)
                        };
                        ui.label(RichText::new(text).monospace().size(9.5).color(fg()));
                    }
                });
            });
            if pretty_metric_name(&series.name) != series.name {
                ui.label(
                    RichText::new(&series.name)
                        .monospace()
                        .size(8.0)
                        .color(dim_fg()),
                );
            }
            ui.add_space(3.0);
            let width = ui.available_width().max(160.0);
            let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 128.0), Sense::hover());
            let painter = ui.painter();
            painter.rect_filled(rect, CornerRadius::same(6), panel_2());
            painter.rect_stroke(
                rect,
                CornerRadius::same(6),
                Stroke::new(1.0, border_soft()),
                egui::StrokeKind::Inside,
            );

            let values: Vec<f64> = series
                .points
                .iter()
                .filter_map(|point| point.value)
                .collect();
            if values.is_empty() {
                painter.text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "No numeric samples available",
                    egui::FontId::proportional(9.5),
                    dim_fg(),
                );
                return;
            }
            let low = values
                .iter()
                .copied()
                .fold(f64::INFINITY, f64::min)
                .min(0.0);
            let high = values
                .iter()
                .copied()
                .fold(f64::NEG_INFINITY, f64::max)
                .max(low + 1.0);
            let plot = egui::Rect::from_min_max(
                egui::pos2(rect.left() + 38.0, rect.top() + 10.0),
                egui::pos2(rect.right() - 8.0, rect.bottom() - 18.0),
            );
            let x = |time: u64| {
                plot.left()
                    + ((time.saturating_sub(start)) as f32
                        / (end.saturating_sub(start)).max(1) as f32)
                        * plot.width()
            };
            let y =
                |value: f64| plot.bottom() - ((value - low) / (high - low)) as f32 * plot.height();

            for fraction in [0.0, 0.5, 1.0] {
                let value = low + fraction * (high - low);
                let py = y(value);
                painter.hline(plot.x_range(), py, Stroke::new(1.0, border_soft()));
                painter.text(
                    egui::pos2(plot.left() - 5.0, py),
                    egui::Align2::RIGHT_CENTER,
                    format_axis(value),
                    egui::FontId::monospace(8.0),
                    dim_fg(),
                );
            }

            let mut segment: Vec<egui::Pos2> = Vec::new();
            let mut previous_time = 0u64;
            for point in &series.points {
                let Some(value) = point.value else {
                    flush(painter, &mut segment);
                    continue;
                };
                let contiguous = segment.is_empty()
                    || step == 0
                    || point.timestamp.saturating_sub(previous_time) <= step.saturating_mul(2);
                if !contiguous {
                    flush(painter, &mut segment);
                }
                segment.push(egui::pos2(x(point.timestamp), y(value)));
                previous_time = point.timestamp;
            }
            flush(painter, &mut segment);

            painter.text(
                egui::pos2(plot.left(), rect.bottom() - 6.0),
                egui::Align2::LEFT_BOTTOM,
                format::clock(start),
                egui::FontId::monospace(8.0),
                dim_fg(),
            );
            painter.text(
                egui::pos2(plot.right(), rect.bottom() - 6.0),
                egui::Align2::RIGHT_BOTTOM,
                format::clock(end),
                egui::FontId::monospace(8.0),
                dim_fg(),
            );

            if let Some(pointer) = response.hover_pos()
                && plot.contains(pointer)
            {
                let fraction =
                    ((pointer.x - plot.left()) / plot.width().max(1.0)).clamp(0.0, 1.0) as f64;
                let target = start as f64 + fraction * (end.saturating_sub(start)) as f64;
                let nearest = series
                    .points
                    .iter()
                    .filter(|point| point.value.is_some())
                    .min_by(|left, right| {
                        (left.timestamp as f64 - target)
                            .abs()
                            .total_cmp(&(right.timestamp as f64 - target).abs())
                    });
                if let Some(point) = nearest
                    && let Some(value) = point.value
                {
                    let px = x(point.timestamp);
                    painter.vline(
                        px,
                        plot.y_range(),
                        Stroke::new(1.0, with_alpha(accent(), 120)),
                    );
                    painter.circle_filled(egui::pos2(px, y(value)), 2.8, accent());
                    painter.text(
                        egui::pos2(plot.right() - 2.0, rect.top() + 6.0),
                        egui::Align2::RIGHT_TOP,
                        format!(
                            "{}  {value:.2} {}",
                            format::short_time(point.timestamp),
                            series.unit
                        ),
                        egui::FontId::monospace(8.5),
                        fg(),
                    );
                }
            }
            ui.add_space(2.0);
        });
}

fn flush(painter: &egui::Painter, segment: &mut Vec<egui::Pos2>) {
    if segment.len() > 1 {
        painter.add(egui::Shape::line(
            std::mem::take(segment),
            Stroke::new(1.4, cyan()),
        ));
    } else {
        segment.clear();
    }
}

fn format_axis(value: f64) -> String {
    let abs = value.abs();
    if abs >= 1_000_000_000_000.0 {
        format!("{:.1}T", value / 1_000_000_000_000.0)
    } else if abs >= 1_000_000_000.0 {
        format!("{:.1}G", value / 1_000_000_000.0)
    } else if abs >= 1_000_000.0 {
        format!("{:.1}M", value / 1_000_000.0)
    } else if abs >= 1_000.0 {
        format!("{:.1}k", value / 1_000.0)
    } else if abs >= 10.0 {
        format!("{value:.0}")
    } else {
        format!("{value:.2}")
    }
}

fn pretty_metric_name(name: &str) -> String {
    let mapped = match name {
        "cpu_avg" => "CPU average",
        "cpu0" => "CPU 0",
        "memory_free_kib" => "Free memory",
        "memory_total_kib" => "Total memory",
        "memory_internal_free" => "Internal free memory",
        "memory" => "Memory",
        "network_throughput" => "Network throughput",
        "pif_aggr_rx" => "Network received",
        "pif_aggr_tx" => "Network sent",
        "vbd_aggr_r" => "Disk read",
        "vbd_aggr_w" => "Disk write",
        "loadavg" => "Load average",
        "sr_io_throughput_total" => "Storage throughput",
        _ => {
            if let Some(rest) = name.strip_prefix("CPU")
                && let Some(index) = rest.find("-avg-freq")
            {
                return format!("CPU {} frequency", &rest[..index]);
            }
            if let Some(rest) = name.strip_prefix("cpu")
                && !rest.is_empty()
                && rest.chars().all(|character| character.is_ascii_digit())
            {
                return format!("CPU {rest}");
            }
            if let Some(rest) = name.strip_prefix("pif_") {
                return format!("PIF {}", rest.replace('_', " "));
            }
            if let Some(rest) = name.strip_prefix("vif_") {
                return format!("VIF {}", rest.replace('_', " "));
            }
            let generic = name.replace('_', " ");
            let mut chars = generic.chars();
            return match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => generic,
            };
        }
    };
    mapped.to_owned()
}
