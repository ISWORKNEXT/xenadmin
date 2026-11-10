# Xen Desktop

A desktop client for XCP-ng and XenServer pools.

The native client uses its own undecorated window chrome: a draggable title bar with workspace tabs, window controls and a light/white theme by default (dark theme is one click away).

Two desktop implementations live in this tree:

- **`native/` — current UI.** Pure Rust with `egui`/`eframe`. No WebView, Tauri, React or shadcn/ui.
- **`desktop/` — legacy UI.** Tauri 2 + React + shadcn/ui, kept for behavioural comparison and local protocol fixtures until the native client reaches parity.

Both use the shared `xen-core` crate (HTTPS transport, XenAPI sessions, inventory, VM actions, tasks, host details and RRD metrics).

The original C# XCP-ng Center source is in Git history at commit `8d01db877`.

## Run the native client

Install a current stable Rust toolchain and, on Linux, the native window/GL development packages for your distribution.

```sh
cd native
cargo run --release
```

Useful commands:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cargo build --release --locked
```

`cargo run --release -- --connect` opens the Add connection dialog on launch. The native client also accepts automation/deep-link flags:

```text
--connect                 open Add connection at startup
--connect-stdin           read one JSON startup connection from stdin
--view overview|vms|hosts|templates|storage|networks|tasks
--host <host-uuid>        open host details after connecting
--host-tab general|cpu|performance|networking|storage|hardware|updates|certificates|health
--vm <vm-uuid>            open the VM detail panel after connecting
--theme light|dark        start with the light (default) or dark theme
--on-top                  keep the window above other windows
```

`--connect-stdin` expects the same shape as the previous client:

```json
{"settings":{"host":"192.168.8.170","username":"root","label":"Lab","read_only":true,"tls":{"bypass_certificate_verification":true}},"password":"..."}
```

A JSON array is also accepted. Do not put credentials in a tracked file.

## Connect

Choose **Add connection**, enter the host address, username and password, then choose **Connect to host**. For a pool member, connect to the pool coordinator. Enable **Bypass TLS certificate verification** only for a self-signed host; it applies to that connection only, HTTPS stays enabled, but certificate identity is not verified.

Passwords and session handles are never saved. Host addresses, usernames, labels and TLS/read-only preferences are stored in the application config directory (`$XDG_CONFIG_HOME/xen-desktop/connections.json` on Linux); the selected light/dark theme is stored in `ui.json` beside it. Passwords are requested again on reconnect.

The native UI also refuses to enable write actions on `10.100.1.11`; the Rust backend independently forces that host read-only before login and blocks VM mutations and task cancellation before any network request.

## Native client feature set

Implemented against `xen-core`:

- Multiple simultaneous pools with independent sessions, an all-connections overview, and connection/host scopes.
- Overview, virtual machines, hosts, templates, storage, networks and activity/tasks views.
- Start, clean shutdown/reboot, force shutdown/reboot, pause, unpause, suspend, resume, clone, disk snapshot and snapshot revert.
- Bulk actions with an explicit target review dialog.
- XenAPI task progress, failure, success, cancellation, and “check again” for interrupted tasks without resubmitting the mutation.
- VM detail panel with snapshots, and a host detail view (General, CPU/memory, Performance, Networking, Storage, Hardware, Updates, Certificates, Health).
- Host RRD graphs for 15 minutes, 1 hour and 24 hours with gap handling.
- Custom title bar with workspace tabs, drag-to-move, resize handles, theme toggle and window controls.
- Light/white theme by default, with a persisted dark-theme toggle.
- Keyboard navigation: `Ctrl+1` … `Ctrl+7`, `Ctrl+K` search, `F5` refresh.

The full parity status and remaining work are in [docs/native-egui-port.md](docs/native-egui-port.md). The client does not yet replace every feature of XCP-ng Center.

## Live verification boundaries

- `192.168.8.170` was verified read-only: authentication, inventory (1 host, 13 VMs, 5 SRs, 2 networks), VM detail, host details and 168 RRD host metrics. No mutations were submitted.
- `10.100.1.11` is production and remains forced read-only in Rust. No production credentials were entered and no production mutation was submitted.
- Local HTTPS fixtures cover protocol behaviour, duplicate pools, task success/failure/cancel, host-detail redaction and read-only enforcement.

## Legacy Tauri client

The previous Tauri/React client is still runnable:

```sh
cd desktop
npm ci
npm run desktop
```

Its architecture and migration notes are in [docs/rust-desktop-migration.md](docs/rust-desktop-migration.md). The optional browser preview bridge is compiled only with the `preview` feature and is absent from production builds.

## Structure

- `native/`: native Rust UI (`egui`/`eframe`).
- `desktop/crates/xen-core`: HTTPS transport, XenAPI sessions, inventory, operations, tasks, host details and performance.
- `desktop/src`, `desktop/src-tauri`: legacy Tauri/React client.
- `docs/`: migration and parity notes.

## License

BSD 2-Clause. See [LICENSE](LICENSE) for the original project notices.
