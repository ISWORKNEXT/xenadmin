# Changelog

## Unreleased

- Add paused starts (including a selected host), memory checkpoints, capability-gated quiesced snapshots and in-place VM-to-template conversion.
- Add supported live vCPU increases, with guest-metrics detection and host/state guards.
- Add shared name, description and exact-tag editing for VMs, templates, snapshots, disks, SRs, networks, hosts and pools, with changed-field validation and optimistic conflict checks.
- Separate VM Resources from metadata editing so CPU, memory and boot changes preserve long descriptions, exact tags and concurrent metadata changes; align creation with shared description validation.
- Add full independent copies of halted VMs/templates to a selected same-connection SR, with retained interrupted-task context and reconciliation without resubmitting the copy.
- Preserve changed copy destinations and attached disks during recovery; retain operation reservations across reconnect.
- Add a real connection-tree Reconnect action and automatic recovery from expired XenAPI sessions.
- Record real-host and native qualification, including complete test-disk checksums, connection interruption, conflict preservation and inventory cleanup; broader parity gaps remain tracked.

- Add VM header and General-tab quick actions for VNC console, disks, network, snapshots and resources; opening Console auto-connects the active running VM.
- Add vCPU, memory and disk-size sliders to the edit/add-disk/resize dialogs, and make the VM edit form scrollable so its footer stays reachable.
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
- Add a read-only Health & diagnostics page with alert, host, storage, pool capacity and broken VM disk/ISO checks.
- Add XenSearch-style `name:`, `host:`, `tag:` and `power_state:` filters with quotes, negation and free text.
- Extend VM editing with BIOS/UEFI firmware, secure boot, boot policy and after-crash behaviour validation.
- Expand health diagnostics with HA configuration, guest tools, management PIFs/bridge duplication, local SR and unattached PBD checks.
- Extend XenSearch with uuid/description/os/kind/connection and vcpus/memory comparator fields.
- Add attach-existing-VDI with owner marking, a searchable VDI catalog, and a bootable VBD toggle.
- Add VM/template grouping by connection/host/power/tag and persistent editable named searches.
- Add VBD slot, multiple-bootable, CD media type and read-only disk consistency diagnostics.
- Add XenSearch boolean AND/OR/NOT operators with parentheses, precedence and implicit AND.
- Expand health checks for HA statefile VDIs, missing guest metrics, default SR and degraded bonds/VLAN PIFs.
- Add CPU/memory overcommit, duplicate VIF MAC, boot-order/device, certificate expiry and pending-update diagnostics.
- Expand diagnostics to 34 checks: services (health check/update sync/WLB), security (legacy SSL/TLS), storage/network types, suspend/crash-dump SRs, PV guests, duplicate names, unattached disks, guest-tools ISO, references and licensing.
- Restore VBD/VDI/PBD/message/VIF/Bond/Certificate event classes so health data stays live.
- Add VM HA restart priority, start/shutdown delay and host affinity editing with validation.
- Add VM tag editing (deduplicated, validated) so XenSearch tag filters and grouping can be curated.
- Add an SR user-disk inventory with attachment/size details and confirmed deletion of unattached writable disks.
- Add data-disk RO/RW mode editing while halted plus hot-plug/unplug on running VMs, with mode/state validation.
- Add offline disk moves between SRs via copy-and-swap, preserving VBD properties and cleaning up the source VDI.
- Fix SR destroy to unplug (not delete) PBDs first so backends such as `file` can locate storage during destruction.
- Add host-local file-backed SR creation, PBD reattach/detach and TRIM via the XAPI trim plugin, with path/name validation.
- Add physical-interface plug/unplug with management-PIF protection, bond and VLAN creation, and VLAN/bond cleanup on network destroy.
- Add PBD plug/unplug with running-VM guards, SR repair (plug detached PBDs) and file-path SR probing with XML result parsing.
- Add a read-only Authentication & RBAC page with redacted external-auth config, subjects and public roles.
- Add SR introduce/forget for file-backed storage with UUID/type/content/shared validation.
- Add a read-only Pool status page for HA, WLB, PVS, VMSS snapshot schedules and DR tasks.
- Add HA enable/disable with shared-heartbeat and failure-tolerance validation.
- Add WLB configure/enable/disable/deconfigure with URL and credential validation.
- Add VM snapshot schedule create/edit/delete with frequency, day, retention and VM-membership validation.
- Add read-only SR-IOV/tunnel discovery with PIF roles, network purposes and host-internal network classification.
- Add PVS site/server/proxy/cache-storage create and remove actions with address, port and cache-size validation.
- Add DR task create/destroy with NFS/iSCSI/HBA settings and SR whitelist validation.
- Add SR-IOV logical network and CHIN (GRE/VXLAN tunnel) creation with management-PIF protection and destroy cleanup.
- Raise worker thread stack size so large inventory/pool-status futures do not overflow.
- Add directory subject role assignment (add/remove) with internal-role/unknown-role guards and local-root protection.
- Add AD authentication enable/disable with domain/service-account validation and confirmation guards.
- Add live VDI migration for running VMs using VDI.pool_migrate, alongside the existing offline copy/swap move.
- Add an embedded SSH text console with credential prompt, ANSI-filtered terminal buffer, key/text forwarding and disconnect controls.
- Add an embedded IronRDP console with credential prompt, framebuffer texture, mouse/keyboard/wheel input and Ctrl+Alt+Del.
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
