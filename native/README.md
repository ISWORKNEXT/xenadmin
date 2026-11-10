# Native desktop client

Pure Rust `egui`/`eframe` client for XCP-ng and XenServer. It uses the shared
`crates/xen-core` crate and has no WebView, React, shadcn/ui or Tauri
dependency.

## Layout

```text
native/
  src/
    main.rs          window, CLI flags, startup connections
    app.rs           eframe app, event draining, actions, shortcuts
    config.rs        connection profile and theme preference paths
    worker.rs        Tokio workspace thread and profile persistence
    console.rs       VNC console session over the XenAPI HTTPS tunnel
    state.rs         UI state, scopes, action gating and workspace helpers
    theme.rs         light/dark palettes, frames and egui visuals
    widgets.rs       badges, cards, resource bars, info groups, toasts
    views/
      sidebar.rs     navigation and connection tree
      chrome.rs      custom title bar/tabs, footer, resize handles
      content.rs     page header, scope toolbar, offline state
      overview.rs    all-connections home
      pool_page.rs   pool summary, hosts, storage, networks, VMs
      inventory.rs   VM/template tables, host cards, storage, networks
      tasks.rs       XenAPI task list
      vm_detail.rs   VM tabs: General, Performance, Storage, Network, HA, Snapshots, Console
      host_details.rs host detail tabs and RRD charts
      dialogs.rs     connect, action review, help, record details
```

## Threading model

`App` runs on the UI thread and never blocks on HTTP. A dedicated Tokio
runtime in `worker.rs` owns the `Arc<Workspace>` and exposes two channels:

- `Command`: connect, refresh, disconnect, remove, VM action, task check/cancel,
  host details, host performance, VM details/perf, console open/input/close,
  shutdown.
- `Event`: workspace snapshot, connect result, action queued, refresh result,
  task result, host detail/perf/VM detail/perf result, console state/frames and
  errors.

The worker polls `Workspace::view()` every second, so inventory and task state
arrive without blocking the renderer. The UI requests a repaint every 200 ms
while work is active and every 800 ms when idle.

## Security

- `xen-core` validates the HTTPS origin, rejects credentials/paths/queries in
  the URL, disables redirects and retries, and applies TLS bypass per connection.
- The window is undecorated; dragging uses `ViewportCommand::StartDrag`, edge
  handles use `BeginResize`, and window actions are explicit viewport commands.
- The UI never displays or logs the password; it is passed to the worker and
  dropped after login.
- Read-only gating is duplicated: disabled controls in `egui` and enforced
  again in Rust before any mutation request.
- `10.100.1.11` is forced read-only in the backend.
- Host detail field selection is allowlisted in `xen-core`, so secrets such as
  `device_config`, license keys and authentication configuration are not
  exported to the UI.

## Run and verify

```sh
cargo run --release
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
```

On Linux the window needs an X11 or Wayland session with OpenGL (glow).

## Live read-only verification

Performed against `192.168.8.170` on 10 November 2026 with a read-only profile:

- authentication and inventory: 1 host, 13 guest VMs, 5 SRs, 2 networks;
- VM table, VM detail panel, storage and network views;
- host details: identity, software, memory/CPU, updates, certificates;
- RRD performance: 168 host metrics, multiple ranges and missing samples.

No VM lifecycle, snapshot, clone or other mutation was submitted. VM mutations
remain verified only by local HTTPS fixtures.

## Known limitations

- Inventory refresh is manual; there is no `event.from` stream yet.
- Tasks are kept in memory for the life of the process.
- Charts use a hand-drawn `egui` painter and intentionally expose the host's
  raw data-source names; custom graph layouts and event overlays are pending.
- Packaging/installers are not configured yet.
