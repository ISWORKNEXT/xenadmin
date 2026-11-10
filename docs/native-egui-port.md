# Native egui UI port

Status as of 10 November 2026. The native client lives in `native/` and uses the
shared `crates/xen-core` crate. It has no Tauri, WebView, React or shadcn/ui
dependency.

## Stack

| Layer | Choice |
| --- | --- |
| Window/event loop | `eframe` 0.37 with the `glow` (OpenGL) backend |
| UI toolkit | `egui` 0.37, `egui_extras` tables |
| Async/runtime | Tokio multi-thread runtime in a dedicated worker thread |
| Protocol | `xen-core`: reqwest + rustls, XenAPI JSON-RPC, RRD HTTP |
| Persistence | `connections.json` and `ui.json` in the platform config directory; no passwords |

## Implemented screens

| Area | Native status | Notes |
| --- | --- | --- |
| Connection dialog | Implemented | Label, host, username, password, read-only, per-connection TLS bypass; production host is locked read-only |
| Infrastructure tree | Implemented | Pools → hosts, VMs, templates, storage and networks; search, expand/collapse and context menus |
| Overview | Implemented | All-connections home with pool/host/VM/task stats, resources and recent VMs |
| Pool page | Implemented | Pool header, hosts, storage usage, networks and VM summary with object links |
| Virtual machines | Implemented | Search, power filter, 25-row pagination, multi-select, bulk actions, connection-scoped rows |
| Templates | Implemented | Search/power filter, detail dialog |
| Hosts | Implemented | Host cards with CPU/memory, “view VMs”, host details |
| Storage | Implemented | Capacity, usage bars, shared/local, hidden/tools/udev SR filtering |
| Networks | Implemented | Bridge, MTU, PIF/VIF counts; internal `xenapi` bridge filtered |
| Activity & tasks | Implemented | Progress, error code, cancel, “check again”; host tasks from all clients via events |
| VM detail | Implemented | General, Performance, Storage, Network, HA and Snapshots tabs; revert with power restore, bulk snapshot deletion, delete VM/snapshot and template provisioning |
| Action review | Implemented | Exact targets, clone/snapshot name, force/revert warnings |
| Host details | Implemented | General, CPU/memory, performance, networking, storage, hardware, updates, certificates, health |
| Host actions | Implemented | Enable/disable, power-on, evacuate, PIF rescan, restart toolstack, reboot, shutdown with confirmation |
| Proxy | Implemented | Per-connection HTTP/SOCKS proxy for XenAPI and RRD traffic; console tunnel is direct |
| Health diagnostics | Implemented | 34 read-only checks across alerts, capacity, storage/network types, HA/redo log, guest tools, disk/VBD references, boot config, services, security, certificates and integrity |
| XenSearch filtering | Implemented | Fields + comparators, AND/OR/NOT and parentheses, grouping, editable named saved searches |

| Alerts | Implemented (basic) | Priority 1–4 XenAPI messages with dismiss; diagnostics/health checks are still separate |
| Authentication / RBAC | Implemented | Auth/config/subjects/roles, subject role assignment, AD enable/disable with confirmation; local root protected |
| Pool status | Implemented | HA enable/disable; WLB configure/enable/disable/deconfigure; VMSS schedule create/edit/delete; PVS site/server/proxy/cache actions; DR task create/destroy; read-only SR-IOV/tunnel/internal-network discovery |
| Storage repository actions | Implemented | Create/introduce/forget/probe/repair/reattach/detach/scan/TRIM/destroy, PBD plug/unplug, user-VDI inventory and safe deletion |
| Network actions | Implemented | Create internal, bond, VLAN, SR-IOV and CHIN (GRE/VXLAN) networks; destroy cleans VLAN/bond/SR-IOV/tunnel records; PIF plug/unplug with management protection |
| Host performance | Implemented | Upstream-style CPU (all cores), memory and network graphs with legends; 15 min / 1 h / 24 h, search, remaining metrics and gap handling |
| Live inventory | Implemented | XenAPI event stream applied to a raw-record cache; heartbeat, reconnect and coordinator redirect |
| VM performance | Implemented | VM RRD with per-vCPU CPU graph, memory/network graphs, remaining metrics and the same ranges |
| VM console | Implemented | VNC HTTPS tunnel; embedded RDP (IronRDP) with framebuffer/input; embedded SSH text console with credential prompt; external RDP fallback |
| VM editing | Implemented | Quick actions + sliders for vCPUs/memory; name/description/tags, BIOS/UEFI, secure boot, boot order/policy, after-crash policy, HA priority/delays/affinity; static memory/CPU while halted; dynamic memory live |
| CD/DVD media | Implemented | List/eject/insert ISO media on existing virtual CD drives from the Storage tab |
| VM network interfaces | Implemented | Add/remove VIFs, plug/unplug, next-free device slot and optional MAC |
| VM virtual disks | Implemented | Add/attach/move (live pool_migrate or offline copy/swap), grow/remove, RO/RW, bootable, hot-plug/unplug, empty CD drives |
| VM start on host | Implemented | Start/resume on a chosen host with missing-VDI/SR preflight and `assert_can_boot_here` |
| Help / migration status | Implemented | Mirrors the previous client's status dialog |
| Window chrome | Implemented | Undecorated custom title bar, object tabs, context toolbar, drag, resize handles and window controls |
| Themes | Implemented | Light/white theme by default with a persisted dark toggle |
| UI scale | Implemented | 110% default, title-bar control and `Ctrl+Plus` / `Ctrl+Minus` / `Ctrl+0`, persisted in `ui.json` |

Keyboard shortcuts: `Ctrl+K` focuses the Infrastructure search, `F5` refreshes
the active scope, `Ctrl+W` closes the active object tab, `Ctrl+Plus` /
`Ctrl+Minus` / `Ctrl+0` adjust the UI scale, and `Esc` closes modals.

## Verification

### Local fixtures

- `cargo test --workspace --locked` from the repository root: 25 core unit
  tests, 4 host-detail integration tests and 5 workspace tests pass.
- `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings` and
  `cargo build -p xen-native` pass.
- UI exercised against local HTTPS fixtures for inventory, duplicate pool UUIDs,
  stale refresh, VM detail, host details, storage, networks, 168-metric RRD gaps,
  and connect/read-only behaviour.

### Live host (read-only)

`192.168.8.170` (`OIIO-Server`, XCP-ng 8.3.0) was accessed with a read-only
profile to verify:

- authentication, inventory and logout;
- 1 host, 13 guest VMs, 5 SRs, 2 networks;
- VM list, long-name truncation, VM detail panel;
- host identity/software/runtime, 4 CPUs, ~64 GiB memory;
- host RRD endpoint: 15-minute query, 168 host metrics, missing-value handling.

No VM lifecycle, snapshot, clone, task cancel or other mutation was submitted.
The app's session was closed after each run.

`10.100.1.11` remains production and forced read-only in Rust; it was not
contacted in this native UI work.

## Design choices

- **No WebView/IPC boundary.** UI and core share Rust types directly.
- **Clone derives on view models.** `xen-core` view structs derive `Clone` so the
  worker can hand snapshots to the UI thread without serialization.
- **Startup/deep-link flags.** `--connect`, `--connect-stdin`, `--view`,
  `--host`, `--host-tab`, `--vm`, `--theme`, `--on-top` support automation and
  screenshots.
- **Painted glyphs.** Navigation, host/server, window-control and empty-state
  marks are painted with `egui` shapes so they do not depend on a particular
  system font.
- **Custom window chrome.** The OS title bar is removed in favour of an
  undecorated title bar with workspace tabs, scope pill, theme toggle and
  minimize/maximize/close controls.
- **Light theme default.** The white palette is the first-run default; the
  title-bar toggle persists the choice in `ui.json`.
- **Charts.** Host performance follows the upstream default graph model: one
  CPU graph with all cores, one memory graph and one network graph, each with a
  colored legend. Remaining XenAPI data sources keep their own bordered cards;
  axes use compact units and gaps are not connected.

## Pending

- New VM provisioning wizard, VM editing, console (VNC/RDP), migration.
- Snapshot disk cleanup, import/export (XVA/OVF), storage/network management.
- Host updates/patches application, HA/DR and scheduling workflows.
- Event stream (`event.from`) instead of manual/interval inventory refresh.
- Coordinator redirect/reconnect handling.
- Task garbage collection and richer alert/diagnostic catalogues.
- Signed installers and release packaging for Linux, Windows and macOS.

## Platform notes

The project is developed on Linux/Wayland. `eframe` uses native X11 or Wayland
windows and OpenGL; CI installs the X11/Wayland/GL development packages. Windows
and macOS builds use the same Rust code but have not been packaged or verified
in this task.
