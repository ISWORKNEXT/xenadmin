# Changelog

## Unreleased

- Add `native/`: a pure-Rust `egui`/`eframe` desktop UI for XCP-ng and XenServer with no Tauri, WebView, React or shadcn/ui dependency.
- Port the current screens: connections, overview, VMs, templates, hosts, storage, networks, tasks, VM detail, action review, host details and RRD performance charts.
- Add a custom undecorated title bar with workspace tabs, drag/resize handling, window controls and a light/dark theme toggle.
- Make the new white theme the default and persist the theme choice.
- Make the main workspace scrollable and render each host-performance metric in its own bordered chart card.
- Add native startup flags for connect, view, host details, VM detail, theme and deep links.
- Keep the Tauri/React client in `desktop/` as a legacy parity reference.

## 0.1.0

- Replace the C# WinForms source and build system with Rust, Tauri and React.
- Add a shadcn/ui desktop workspace for host, VM, storage and network inventory.
- Add per-connection TLS certificate bypass, search, filtering, details and manual refresh.
- Keep XenAPI credentials and sessions in process memory.

The previous XCP-ng Center changelog is available in Git history at `8d01db877`.
