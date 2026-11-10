# Feature migration status

Updated 10 November 2026. The original C# source is in Git history at `8d01db877`.
The current UI is the native `egui` client in `native/`; see
[`native-egui-port.md`](native-egui-port.md). The Tauri/React client in `desktop/`
remains as a parity reference.

## Implemented in the native client (and the legacy React client)

| Area | Current behavior | Verification |
| --- | --- | --- |
| Multiple pools | Independent sessions, all-connections overview, pool/host scopes and connection columns | Local HTTPS tests with distinct pools and identical VM UUIDs/names |
| Connection failure | Failed refresh preserves stale inventory and does not block another pool | Local HTTPS integration test |
| Saved profiles | Save host, username, label, TLS and read-only preferences. Never save passwords or session handles | Application settings review and UI check |
| Inventory | Hosts, guest VMs, templates, snapshots, storage and networks | Live read-only XenAPI on 192.168.8.170 |
| TLS bypass | Explicit per-connection setting | Strict rejection and bypass success on 192.168.8.170 |
| VM lifecycle | Start, shutdown, reboot, force shutdown/reboot, pause, unpause, suspend and resume | Local HTTPS task workflow tests. No live mutations |
| Clone | Async VM clone preserving the source configuration | Implemented. Not exercised against a live host |
| Snapshots | List, create disk snapshot, revert. Revert does not automatically restore running power state | Implemented. Not exercised against a live host |
| Bulk actions | Review exact VM names and connection addresses before submission | Connection-scoped request model and UI check |
| Tasks | Progress, success, host failure, cancellation, interrupted status and checking the original task again | Local HTTPS success/failure/cancel tests |
| Production policy | 10.100.1.11 cannot perform mutations even if a client submits read_only=false | Unit tests verify forced policy and that blocked actions send no network request |

The workspace deduplicates pools by their XenAPI pool UUID. Connecting to another member of an already connected pool does not duplicate all of its objects. Current connection-to-coordinator redirection remains manual.

## Pending migration

| Area | Original source reference | Work remaining |
| --- | --- | --- |
| New VM wizard | XenModel/Actions/VM/CreateVMAction.cs, CreateVMFastAction.cs | Template clone/provision workflow, disk/network mapping, CPU/memory, boot and install options |
| VM editing | XenAdmin/SettingsPanels, XenModel/Actions/VM | Rename, CPU/memory, boot, tools, tags and advanced settings |
| Console | XenAdmin/ConsoleView, XenModel/Network/HTTPHelper.cs | VNC CONNECT bridge, keyboard/clipboard and RDP replacement |
| VM migration | VMMigrateAction.cs, VMCrossPoolMigrateAction.cs | Eligibility checks, destination host/storage/network and transfer progress |
| Snapshot cleanup | VMSnapshotDeleteAction.cs, VMDestroyAction.cs | Ownership-aware VBD/VDI cleanup. VM.destroy alone is not an equivalent replacement |
| Storage management | XenModel/Actions/SR, VDI, VBD | Add, scan, repair, attach, resize, move and delete disks/repositories |
| Network management | XenAdmin/Wizards/NewNetworkWizard*, XenModel/Actions/VIF | Network creation, bonds, VLANs and interface edits |
| Import/export | XenOvfApi, ImportVmAction.cs, ExportVmAction.cs | XVA/OVF streaming, disk formats, cancellation and partial cleanup |
| Metrics | XenModel/XenSearch/MetricUpdater.cs, CustomDataGraph | RRD time series, history and server clock offset |
| Events | XenModel/XenObjectDownloader.cs | event.from bootstrap/cursor, resync and continuous inventory updates |
| Coordinator failover | XenModel/Network/XenConnection.cs | Redirect handling, pool coordinator change and reconnect |
| Alerts, permissions | XenModel/Alerts, XenAdmin/Diagnostics | Notifications, richer permission feedback and diagnostics |
| Host administration | XenModel/Actions/Host, Pool, Updates | Maintenance, updates, pool membership and host operations |
| HA, DR, scheduling | XenAdmin/Wizards/HAWizard*, DRWizards, XenModel/Actions/VMSS | Full workflows and recovery scenarios |
| Advanced hardware | XenModel/Actions/USB, GPU-related actions | Passthrough, GPU, vTPM and platform-specific checks |
| Packaging | installer-xcp-ng | Signed native release artifacts and installers for selected platforms |

## Live host boundaries

`192.168.8.170` has been used for authentication, reads, refresh, duplicate-pool checks and logout. VM mutations have not been run there.

`10.100.1.11` is production. An offline read-only profile was prepared without credentials. During UI review, the connection was observed online with 76 guest VMs and read-only controls. The agent did not enter its credentials or submit a production mutation. UI review confirmed that start, shutdown, reboot, snapshots, clone and force actions are disabled.

The code does not yet provide full XCP-ng Center feature parity. Enabled controls invoke Rust/XenAPI behavior; pending features are described here instead of represented by working-looking buttons.
