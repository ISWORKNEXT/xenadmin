# Unit M4 — Server and Pool menus, networking

Read `docs/supervision/CHARTER.md` first. It is binding. Backend changes
are allowed. The dev host is a full-freedom lab: reboot, toolstack
restart and maintenance mode are allowed; do not power the host off.

## Reference (`/home/tinnerkun/Desktop/xen/upstream-xenadmin/XenAdmin/`)

- Dialogs: `EvacuateHostDialog`, `ExitMaintenanceModeDialog`,
  `ControlDomainMemoryDialog`, `ChangeServerPasswordDialog`,
  `BondProperties`, `NetworkingProperties*`.
- Wizards: `NewNetworkWizard.cs` + `NewNetworkWizard_Pages/`
  (NetWTypeSelect, NetWName, NetWDetails, NetWBondDetails,
  NetWChinDetails, NetWSriovDetails), `NewPoolDialog`/`NewPoolCommand`.
- Commands: Server menu and Pool menu items in
  `docs/supervision/upstream-shell.txt` and `menu-status.tsv`
  (rows still `disabled-not-implemented` for Server/Pool/network).
- TabPages: `NICPage.cs`, `NetworkPage.cs` (host).

## Deliver

1. **Server menu** with upstream dialogs and enable rules: Reconnect,
   Disconnect, Reboot, Shut Down (code path only — never run it live),
   Power On (if host has power-on configured), Restart Toolstack, Enter /
   Exit Maintenance Mode (EvacuateHostDialog listing VMs and where they
   go; ExitMaintenanceModeDialog to bring VMs back), Back Up… /
   Restore From Backup… (host.get_backup / host restore over HTTP, file
   dialogs), Control Domain Memory…, Change Server Password…,
   Remove Crash Dumps, Server Status Report entry if it exists
   (otherwise disabled), Properties (M2a).
2. **Pool menu**: New Pool… (dialog: name, coordinator, members — create
   pool from a standalone host = set name; join is `untested-lab`), Add
   Server to Pool / Remove Server from Pool / Make Standalone (port the
   flows, `untested-lab`), Rotate Pool Secret, Disconnect, Properties.
3. **New Network wizard** on the shared wizard frame: type selection
   (External = VLAN, Single-Server Private, Cross-Server Private (CHIN,
   untested-lab), Bonded Network, SR-IOV if supported), name, details
   pages per type, finish. Reuse the existing VLAN/bond/SR-IOV backends;
   add internal network creation. Network Properties dialog (name,
   description, MTU, auto-add to new VMs, NIC/VLAN for external).
   Remove network with upstream guards.
4. **NICs tab**: Create Bond… (bond wizard page), Delete Bond, Rescan
   (`PIF.scan`), plus columns already there.
5. **Host Networking tab**: "Configure…" for IP Address Configuration
   (management interface and secondary interfaces: DHCP/static, IP,
   mask, gateway, DNS) per upstream `NetworkingProperties`. Changing the
   management interface IP is **code path only** on the lab (do not
   change .170's management IP); test with a secondary interface on an
   internal network if possible, else code path.
6. Update `docs/supervision/menu-status.tsv` for every changed row.

## Accept

- fmt, clippy, tests.
- Screenshots in `docs/supervision/evidence/M4/`: Evacuate dialog, New
  Network type page, Control Domain Memory dialog, NICs tab, IP
  configuration dialog.
- Live on .170: Restart Toolstack (wait for XenAPI to answer again and
  the app to reconnect); Enter Maintenance Mode with no running VMs
  moved off (single host: upstream shows the "will shut down/suspend"
  choices — use a halted-only state or let the dialog report what it
  would do, then Cancel if it requires stopping pre-existing running
  VMs) and Exit; create a Single-Server Private network
  `zz-parity-M4-net`, rename it in Properties, then remove it; Change
  Server Password: code path only. Host Back Up… to a local file of the
  host config (small) and confirm the file is non-empty, then delete it.
