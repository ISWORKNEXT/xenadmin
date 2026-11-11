# Roadmap and unit status

Status lives in GitHub issues (label `parity`), updated with `gh`:
`gh issue list --label parity`. This file only defines scope and order.
Issues: S1 #1, S2 #2, S3 #3, S4 #4, S5 #5, M2 #6, M3 #7, M4 #8, M5 #9, M6 #10, M7 #11, M8 #12, M9 #13.

## M1 — Shell matches XCP-ng Center (UX first)

| Unit | Scope |
|---|---|
| S1 | Window frame: OS decorations, menu bar with the full upstream menu tree, toolbar, status bar. Existing actions wired; missing ones disabled |
| S2 | Navigation pane: Infrastructure tree per `MainWindowTreeBuilder`, View-menu filters, nav buttons (Infrastructure, Objects, Organization Views, Saved Searches, Notifications); selection drives the right pane |
| S3 | Per-object tab sets per `MainWindow.GetNewTabPages` (Home, General, Memory, Storage, Networking, NICs, GPU, USB, Console, Performance, Snapshots, Search; rules in `upstream-tabs-tree.md`); existing detail content rehomed |
| S4 | Context menus built from the same command set as the menu bar, with upstream enable rules and disabled-reason tooltips |
| S5 | Notifications: Alerts, Updates, Events (task history with filters) |

## M2 — VM menu and VM tabs complete
Properties dialog (vertical tabs: General, Custom Fields, CPU, Boot Options,
Start Options, Alerts, Home Server, GPU, Advanced), Copy/Move VM wizards,
snapshot tree view and "Save/Export snapshot", install tools, vApp and
snapshot-schedule assignment, Delete VM dialog with disk/snapshot choice.

## M3 — Storage
New Storage wizard for every SR type (NFS, iSCSI, HBA, SMB/CIFS, ISO, local),
virtual disk New/Attach/Move/Properties, Repair, Reclaim, Reattach.

## M4 — Server and Pool menus
Host properties, Back Up/Restore, Maintenance Mode with evacuation dialog,
Control Domain Memory, Change Password, Restart Toolstack, Power On,
New Pool, Add to/Remove from Pool, Make standalone, Pool properties.
Network: New Network wizard (external/VLAN, internal, bond, SR-IOV, CHIN), NICs tab.

## M5 — Import / Export
Import wizard (XVA, OVF/OVA, disk image), Export wizard, server list
import/export, search import.

## M6 — Organization and Tools
Folders, tags, custom fields, saved searches, Objects view, Tools > Options
(all pages), Help menu items.

## M7 — Monitoring
Performance tab graph editing and persistence, alert thresholds, events
filters, resource report.

## M8 — Pool services
HA configure/disable wizard, WLB, DR wizards, VM snapshot schedules, vApps,
PVS, updates/patching.

## M9 — Final pass
Click-through of every menu item and tab against `.170`, Windows GUI runtime
check, console (VNC/RDP/SSH) parity, release.
