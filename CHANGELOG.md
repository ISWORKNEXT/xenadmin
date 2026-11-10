# Changelog

## Unreleased

- Rework the native UI into an XCP-ng Center-style Infrastructure tree with object tabs, a contextual toolbar and a status bar.
- Add a custom undecorated title bar, object tabs, drag/resize handling, window controls and a persisted light/dark theme toggle.
- Make the main workspace scrollable and render each host-performance metric in its own bordered chart card.
- Add native startup flags for connect, view, host details, VM detail, theme and deep links.
- Add the `ui-ux-pro-max` UI/UX design skill under `.opencode/skills/`.
- Move the shared XenAPI core to `crates/xen-core` and remove the legacy Tauri/React desktop tree.

## 0.1.0

- Replace the C# WinForms source with a native Rust desktop client.
- Add pool, host, VM, template, storage and network inventory.
- Add per-connection TLS certificate bypass, search, filtering, details and manual refresh.
- Keep XenAPI credentials and sessions in process memory.

The previous XCP-ng Center changelog is available in Git history at `8d01db877`.
