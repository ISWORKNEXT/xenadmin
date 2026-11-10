# Changelog

## Unreleased

- Rework the native UI into an XCP-ng Center-style Infrastructure tree with object tabs, a contextual toolbar and a status bar.
- Add a custom undecorated title bar, object tabs, drag/resize handling, window controls and a persisted light/dark theme toggle.
- Increase default typography/readability (110% UI scale) and add a title-bar zoom control plus `Ctrl+Plus` / `Ctrl+Minus` / `Ctrl+0`.
- Add a dedicated pool overview page and contextual VM/host/snapshot toolbar actions; truncate long object names in tabs and the tree.
- Render host performance as upstream-style multi-series graphs: CPU cores in one chart, memory and network separately, with legends and compact axes.
- Add VM read model (disks/VDIs/SRs, VIFs/networks, HA/platform/tools fields) and VM detail tabs for General, Performance, Storage, Network, HA and Snapshots.
- Add live VM RRD performance with the same multi-series graph model.
- Add New VM from template: clone, provision declared disks, remove the provisioning XML and refresh the inventory.
- Add delete VM/template/snapshot with owner VBD/VDI cleanup, suspend-image removal, optional snapshot cascade and confirmation of disk deletion.
- Open `--vm` deep links as a template or snapshot when the UUID belongs to one, and close object tabs whose objects were deleted.
- Add an embedded VNC console tab: HTTPS CONNECT tunnel with the XenAPI session, live framebuffer with partial texture uploads, mouse/wheel and keyboard input.
- Add VM editing for name, description, vCPU limits and the static memory maximum; CPU/memory changes require a halted VM and clamp dynamic memory to the new maximum.
- Replace periodic inventory polling with the XenAPI event stream and a local raw-record cache, so changes from other clients appear automatically.
- Add start-on-host and resume-on-host actions with an `assert_can_boot_here` precheck and host picker.
- Extend VM editing with dynamic memory limits (live when permitted) and HVM boot order.
- Add CD/DVD media management: list ISO images from ISO SRs, insert a selected image or eject the current media.
- Show host tasks from the event stream in Activity & tasks, including tasks started by other clients.
- Add VM network interface management: create on the next free slot with an optional MAC, remove, plug and unplug.
- Add virtual disk management: create on a chosen SR, grow, and remove disks (optionally deleting the VDI) on halted VMs.
- Add empty CD/DVD drive creation and two-way console clipboard actions (copy guest, paste from desktop).
- Restore the running power state after reverting a snapshot taken while the VM was running.
- Preflight VM starts against missing VDIs and storage repositories without attached PBDs.
- Route printable console input through Unicode keysyms so non-US keyboard layouts work better.
- Add host administration actions: enable/disable, evacuate, restart toolstack, reboot and shutdown behind confirmation.
- Read guest IPv4 addresses from guest metrics and launch an external xfreerdp session from the console tab.
- Collect priority XenAPI message records into pool alerts and allow dismissing them.
- Add checkbox selection, select-all and confirmed bulk deletion in the VM Snapshots tab.
- Add storage repository scan/destroy and XenAPI network create/destroy actions.
- Add host power-on and physical-interface rescan actions.
- Add optional per-connection HTTP/SOCKS proxy settings with URL validation; XenAPI/RRD traffic uses the proxy.
- Add a 15-second heartbeat, in-memory-password reconnect and automatic `HOST_IS_SLAVE` coordinator redirect.
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
