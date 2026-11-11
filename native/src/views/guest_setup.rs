use xen_core::guest_setup::{GuestSetup, GuestSupport};

pub fn fields(ui: &mut egui::Ui, setup: &mut GuestSetup, support: &GuestSupport) {
    ui.horizontal(|ui| {
        ui.label("IP family");
        ui.selectable_value(&mut setup.dual_stack, false, "IPv4");
        ui.selectable_value(&mut setup.dual_stack, true, "Dual stack");
    });
    if !setup.dual_stack {
        setup.ipv6.clear();
        setup.ipv6_prefix.clear();
        setup.ipv6_gateway.clear();
    }
    for (label, value) in [
        ("IPv4 address", &mut setup.ipv4),
        ("Prefix length", &mut setup.prefix),
        ("Gateway", &mut setup.gateway),
        ("DNS 1", &mut setup.dns1),
        ("DNS 2", &mut setup.dns2),
    ] {
        ui.horizontal(|ui| {
            ui.label(label);
            ui.text_edit_singleline(value);
        });
    }
    if setup.dual_stack {
        ui.label("Optional IPv6: supply all three fields or leave all empty.");
        for (label, value) in [
            ("IPv6 address", &mut setup.ipv6),
            ("IPv6 prefix", &mut setup.ipv6_prefix),
            ("IPv6 gateway", &mut setup.ipv6_gateway),
        ] {
            ui.horizontal(|ui| {
                ui.label(label);
                ui.text_edit_singleline(value);
            });
        }
    }
    if support.username {
        ui.horizontal(|ui| {
            ui.label("Login username");
            ui.text_edit_singleline(&mut setup.username);
        });
    }
    if support.ssh_keys {
        ui.label("SSH public keys (one per line)");
        ui.add(
            egui::TextEdit::multiline(&mut setup.ssh_keys)
                .desired_rows(3)
                .desired_width(f32::INFINITY),
        );
    }
    password(ui, setup);
    ui.label("Guest applies settings on next boot. Leave password empty to omit a new password.");
}
pub fn password(ui: &mut egui::Ui, setup: &mut GuestSetup) {
    for (label, value) in [
        ("Initial password", &mut setup.password),
        ("Confirm password", &mut setup.confirm),
    ] {
        ui.horizontal(|ui| {
            ui.label(label);
            ui.add(egui::TextEdit::singleline(value).password(true));
        });
    }
}
