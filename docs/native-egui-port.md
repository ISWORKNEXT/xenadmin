# Native egui UI port

Status as of 10 November 2026. The native client lives in `native/` and shares
`desktop/crates/xen-core` with the legacy Tauri client. It has no Tauri, WebView,
React or shadcn/ui dependency.

## Stack

| Layer | Choice |
| --- | --- |
| Window/event loop | `eframe` 0.37 with the `glow` (OpenGL) backend |
| UI toolkit | `egui` 0.37, `egui_extras` tables |
| Async/runtime | Tokio multi-thread runtime in a dedicated worker thread |
| Protocol | `xen-core`: reqwest + rustls, XenAPI JSON-RPC, RRD HTTP |
| Persistence | `connections.json` in the platform config directory; no passwords |

## Implemented screens

| Area | Native status | Notes |
| --- | --- | --- |
| Connection dialog | Implemented | Label, host, username, password, read-only, per-connection TLS bypass; production host is locked read-only |
| Sidebar / scopes | Implemented | All connections, pool, host; expandable host tree; refresh, reconnect, disconnect, remove |
| Overview | Implemented | Pool/host/VM/task stats, connection cards, memory and storage allocation, recent VMs |
| Virtual machines | Implemented | Search, power filter, 25-row pagination, multi-select, bulk actions, connection-scoped rows |
| Templates | Implemented | Search/power filter, detail dialog |
| Hosts | Implemented | Host cards with CPU/memory, “view VMs”, host details |
| Storage | Implemented | Capacity, usage bars, shared/local, hidden/tools/udev SR filtering |
| Networks | Implemented | Bridge, MTU, PIF/VIF counts; internal `xenapi` bridge filtered |
| Activity & tasks | Implemented | Progress, error code, cancel, “check again” without resubmission |
| VM detail | Implemented | Status, configuration, snapshots, revert, start/shutdown/more actions |
| Action review | Implemented | Exact targets, clone/snapshot name, force/revert warnings |
| Host details | Implemented | General, CPU/memory, performance, networking, storage, hardware, updates, certificates, health |
| Host performance | Implemented | 15 min / 1 h / 24 h, metric search, 12 charts per page, gap handling |
| Help / migration status | Implemented | Mirrors the previous client's status dialog |
| Window chrome | Implemented | Undecorated custom title bar, workspace tabs, drag, resize handles and window controls |
| Themes | Implemented | Light/white theme by default with a persisted dark toggle |

Keyboard shortcuts: `Ctrl+1` … `Ctrl+7` switch views, `Ctrl+K` opens the VM
search, `F5` refreshes the visible scope, `Esc` closes modals.

## Verification

### Local fixtures

- `cargo test --workspace --locked` in `desktop/`: 25 core unit tests, 4 host-detail
  integration tests and 5 workspace tests pass. Two native command tests also run
  with `--features preview`.
- Native client: `cargo fmt`, `cargo clippy --all-targets -- -D warnings` and
  `cargo build` pass.
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

## Deliberate differences from the Tauri client

- **No WebView/IPC boundary.** UI and core share Rust types; there is no HTTP
  preview bridge or renderer command serializer in production.
- **Clone derives on view models.** `xen-core` view structs now derive `Clone`
  so the worker can hand snapshots to the UI thread without serialization.
- **Startup/deep-link flags.** `--connect`, `--connect-stdin`, `--view`,
  `--host`, `--host-tab`, `--vm`, `--on-top` support automation and screenshots.
- **Painted glyphs.** Navigation, host/server, window-control and empty-state
  marks are painted with `egui` shapes so they do not depend on a particular
  system font.
- **Custom window chrome.** The OS title bar is removed in favour of an
  undecorated title bar with workspace tabs, scope pill, theme toggle and
  minimize/maximize/close controls.
- **Light theme default.** The white palette is the first-run default; the
  title-bar toggle persists the choice in `ui.json`.
- **Charts.** A small painter-based line chart replaces the planned RRD chart
  component; each XenAPI data source gets its own bordered card, axes use compact
  units and gaps are not connected.

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
