# Xen Desktop

A native desktop client for XCP-ng and XenServer, written in Rust with
`egui`/`eframe`. There is no WebView, Tauri, React or shadcn/ui dependency.

The app uses an XCP-ng Center-style workspace: an **Infrastructure tree** on the
left (pools → hosts, virtual machines, templates, storage, networks) and
**object tabs** on the right, with a contextual toolbar and status bar. The
visual language is new: a custom undecorated title bar, a white/light theme by
default and a persisted dark-theme toggle.

## Features

- Multiple pools with independent sessions, an all-connections home view and
  connection/host scopes.
- Infrastructure tree with search, context menus and object tabs.
- Pool overview page with hosts, storage, networks and VM summary.
- VM lifecycle: start (including a missing-VDI/SR preflight and start/resume on
  a chosen host), clean
  shutdown/reboot, force shutdown/reboot, pause, unpause, suspend, resume,
  clone, disk snapshot, snapshot revert/delete (restoring a running snapshot's
  power state), bulk snapshot selection/deletion and delete VM with owned-disk
  and optional snapshot cleanup.
- Create a VM from a template: clone, provision declared disks, refresh the
  inventory and track progress in Activity & tasks.
- Edit a VM's name, description, firmware (BIOS/UEFI), secure boot, boot order,
  boot policy and after-crash behaviour; change vCPUs and static memory while
  halted, and dynamic memory while running when allowed.
- Change or eject CD/DVD media from the VM Storage tab, with a searchable list
  of ISO images from ISO storage repositories.
- Add, remove, plug and unplug virtual network interfaces from the VM Network tab.
- Add, resize and remove virtual disks while the VM is halted, and create empty
  CD/DVD drives.
- Host actions menu for enable/disable, power-on, evacuate, rescan physical
  interfaces, restart toolstack, reboot and shutdown, with explicit confirmation
  for disruptive actions.
- Priority pool alerts from XenAPI message records, shown on the pool page with
  a dismiss action.
- Storage repository scan/destroy actions and XenAPI network create/destroy.
- Bulk actions with an explicit target review dialog.
- Live inventory updates from the XenAPI event stream with a local raw-record
  cache; manual refresh remains available as a fallback.
- A 15-second heartbeat with automatic session reconnect (password kept only in
  process memory) and automatic `HOST_IS_SLAVE` coordinator redirects.
- XenAPI task progress, failure, success, cancellation and “check again” for
  interrupted tasks without resubmitting the mutation.
- Activity & tasks also lists host tasks from the event stream, including
  operations started by other clients.
- VM detail tabs: General, Performance (VM RRD), Storage, Network, HA, Snapshots and Console.
- Embedded VNC console over the host's HTTPS CONNECT tunnel, with live
  framebuffer updates, mouse, scroll wheel, keyboard input (including typed
  Unicode characters), Ctrl+Alt+Del, clipboard exchange and an external RDP
  launcher using the guest IPv4 address.
- Host detail page: General, CPU/memory, Performance, Networking, Storage,
  Hardware, Updates, Certificates, Health.
- Host performance graphs with all CPU cores in one chart, plus memory and
  network graphs, for 15 minutes, 1 hour or 24 hours, with gap handling.
- Light/white theme by default; dark theme toggle persisted in `ui.json`.
- Adjustable UI scale (110% by default) from the title bar or with `Ctrl+Plus`, `Ctrl+Minus`, `Ctrl+0`.
- Keyboard navigation: `Ctrl+K` tree search, `F5` refresh, `Ctrl+W` close tab.

The native client is still growing toward full XCP-ng Center parity. See
[upstream parity gap](docs/upstream-parity-gap.md) and
[native port notes](docs/native-egui-port.md).

## Run

Install a current stable Rust toolchain and, on Linux, the native window/OpenGL
development packages for your distribution.

```sh
cargo run --release -p xen-native
```

Useful commands (from the repository root):

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --release -p xen-native --locked
```

The executable is written to `target/release/xen-native`.

### Startup/deep-link flags

```text
--connect                 open Add connection at startup
--connect-stdin           read one JSON startup connection from stdin
--view overview|vms|hosts|templates|storage|networks|tasks
--host <host-uuid>        open host details after connecting
--host-tab general|cpu|performance|networking|storage|hardware|updates|certificates|health
--vm <vm-uuid>            open the VM detail tab after connecting
--vm-tab general|performance|storage|network|ha|snapshots|console
--health                  open the pool Health & diagnostics page
--search QUERY            prefill the VM/template search (e.g. power_state:running)
--theme light|dark        start with the light (default) or dark theme
--on-top                  keep the window above other windows
```

`--connect-stdin` expects:

```json
{"settings":{"host":"192.168.8.170","username":"root","label":"Lab","read_only":true,"tls":{"bypass_certificate_verification":true}},"password":"..."}
```

A JSON array is also accepted. Do not put credentials in a tracked file.

## Connect

Choose **Add connection**, enter the host address, username and password, then
choose **Connect to host**. For a pool member, connect to the pool coordinator.
Enable **Bypass TLS certificate verification** only for a self-signed host; it
applies to that connection only, HTTPS stays enabled, but certificate identity
is not verified.

Passwords and session handles are never saved. Host addresses, usernames,
labels and TLS/read-only preferences are stored in
`$XDG_CONFIG_HOME/xen-desktop/connections.json` on Linux; the selected theme is
stored in `ui.json` beside it. Passwords are requested again on reconnect.

The native UI also refuses to enable write actions on `10.100.1.11`; the Rust
backend independently forces that host read-only before login and blocks VM
mutations and task cancellation before any network request.

## Live verification boundaries

- `192.168.8.170` was verified read-only: authentication, inventory (1 host,
  13 VMs, 5 SRs, 2 networks), VM detail, host details and 168 RRD host metrics.
  No mutations were submitted.
- `10.100.1.11` is production and remains forced read-only in Rust. No
  production credentials were entered and no production mutation was submitted.
- Local HTTPS fixtures cover protocol behaviour, duplicate pools, task
  success/failure/cancel, host-detail redaction and read-only enforcement.

## Structure

- `crates/xen-core`: HTTPS transport, XenAPI sessions, inventory, operations,
  tasks, host details and RRD performance.
- `native/`: native Rust UI (`egui`/`eframe`).
- `docs/`: native architecture, upstream gap analysis and host-info notes.
- `.opencode/skills/ui-ux-pro-max/`: project UI/UX design skill (MIT).

Historical C# XCP-ng Center source remains available in Git history at commit
`8d01db877`.

## License

BSD 2-Clause. See [LICENSE](LICENSE) for the original project notices.
