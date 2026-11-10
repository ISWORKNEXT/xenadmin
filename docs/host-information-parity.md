# Host information parity checklist

## Task 1 scope and status key

Read-only comparison of the original XCP-ng Center source at git `8d01db877` with the current Xen Desktop tree. Host information only; exclude VM details and host-changing actions. Statuses distinguish **Existing summary**, **Draft, unverified**, **Missing**, and **Host/version dependent**.

No live host calls were made. Production `10.100.1.11` was not contacted or changed. Draft means source is present in the working tree; it is not evidence of compilation, UI integration, runtime correctness, or server compatibility.

## Comparison at the end of Task 1

This table is the Task 1 baseline. Task 2 implementation and verification are recorded below. UI integration and real-host qualification remain pending.

| Original source / symbol | Host information and read inputs | Current app / draft status and gap |
| --- | --- | --- |
| `XenAdmin/TabPages/GeneralTabPage.cs`: `GenerateGeneralBox()`, `GenerateVersionBox()` | Identity/description, tags/folder, pool coordinator, liveness/enabled/maintenance, VM autostart, TLS verification, iSCSI IQN, local/remote syslog, host/agent uptime, AD service name, software product/version/build/date/DB version. Relevant fields include `host.enabled`, `software_version`, `external_auth_service_name`, TLS/pool settings and version-gated iSCSI IQN (`host.iscsi_iqn` on Kolkata+, older `other_config['iscsi_iqn']`). | **Existing summary:** name, address, enabled, product/version. Other General values are **Missing** from current UI. **Draft, unverified:** `host.rs` general/software groups. Verify each field/helper and version gate; draft uptime currently uses `other_config` times and `host.get_servertime`, which must match original `Host.BootTime()` / `AgentStartTime()` semantics. |
| `XenAdmin/TabPages/GeneralTabPage.cs`: `GenerateMemoryBox()`; `XenModel/XenAPI-Extensions/Host.cs`: `HostMemoryString()`, `XenMemoryString()`, `ResidentVMMemoryUsageString()`; `XenAdmin/TabPages/BallooningPage.cs` host rows | Host physical memory, available-for-placement, Xen/Dom0 memory and resident-VM aggregate. Legacy formulas: available = `max(0, memory_total - tot_dyn_min() - xen_memory_calc())`; Xen memory includes host/resident VM overhead and Dom0 `VM_metrics.memory_actual`. | **Existing summary:** total/free and arithmetic used. **Draft, unverified:** total/free plus `host.memory_overhead`. This is **not equivalent** to `memory_available_calc()` or `xen_memory_calc()`; draft lacks placement headroom via `tot_dyn_min`. Per-VM Ballooning rows are excluded; host aggregates remain required. |
| `XenAdmin/TabPages/GeneralTabPage.cs`: `GenerateCPUBox()` / `Helpers.GetCPUProperties()` | `host.host_CPUs` → `host_cpu`: number, model, speed, vendor and CPU properties; sort/group identical CPUs. | **Existing summary:** logical CPU count. **Draft, unverified:** `cpu` section. Confirm field coverage, order/grouping, and missing-vs-zero semantics. |
| `XenAdmin/TabPages/GeneralTabPage.cs`: `GenerateInterfaceBox()`; `XenAdmin/TabPages/NICPage.cs`: `updateList()` / `PIFRow.Update()`; `XenAdmin/TabPages/NetworkPage.cs`: `PIFRow` | Hostname and management interfaces; physical NIC name/MAC/link/speed/duplex/vendor/device/PCI path, FCoE and SR-IOV; network purpose, IP configuration/address/subnet/gateway/DNS. Sources: `host.PIFs`, `PIF`, `PIF_metrics`, `network`, bond/PCI records. FCoE/SR-IOV and secondary-management details are version/feature dependent. | **Missing** in current host-level UI (global Networks page is pool inventory). **Draft, unverified:** networking section joins PIFs/metrics/networks/bonds/PCI. Gaps to resolve: FCoE, SR-IOV state/reboot/logical-PIF/remaining VF count, driver/offload details, and exact PIF inclusion/filtering. |
| `XenAdmin/TabPages/PhysicalStoragePage.cs`: `updateList()` / `SRRow.UpdateDetails()` | Host PBD→SR attachments; name, description, type, shared, physical size/utilization and virtual allocation; excludes hidden/tools SRs. Sources: `PBD.host`, `PBD.SR`, SR fields and state. | **Existing summary:** global SR name/type/capacity/usage/shared, not host attachment/status. **Draft, unverified:** `storage` joins PBD/SR. Verify hidden/tools filtering, PBD/SR status and distinct `physical_size`, `physical_utilisation`, `virtual_allocation`; never expose `PBD.device_config` wholesale. |
| `XenAdmin/TabPages/GpuPage.cs`: `RebuildList()` / `GpuSettings`; `XenAdmin/TabPages/UsbPage.cs`: `HostUsbRow.UpdateDetails()` | Host PGPU/PCI and supported/enabled vGPU types; USB path/description/passthrough. Placement edits/toggle are excluded; USB attached-VM detail is VM information and excluded. | **Missing** in current UI. **Draft, unverified:** GPU/PCI/USB sections. Verify host ownership, vGPU feature gates, grouping and PUSB field names. |
| `XenAdmin/TabPages/GeneralTabPage.cs`: `GenerateMultipathBox()` / `GenerateMultipathBootBox()`, `CheckServerUpdates()` / `CheckHostUpdatesRequiringReboot()` | Host storage path health/boot path counts; restart/toolstack warnings derived from patch/update guidance and applied time versus boot/agent start. Legacy path: `Pool_patch`; Ely+ uses `host.updates_requiring_reboot` / `Pool_update` with legacy toolstack checks. | **Missing** in current UI. **Draft, unverified:** update list exists, but list presence is not equivalent to restart warnings. No multipath status equivalent identified. Preserve old/new API rules and report these gaps explicitly. |
| `XenAdmin/TabPages/GeneralTabPage.cs`: `GenerateCertificateBox()` / `GetCertificateType()` | Host certificate type, validity dates and fingerprint; display starts at Stockholm+, and type/actions vary by server/API version. Install/reset actions are excluded. | **Missing** in current UI. **Draft, unverified:** certificate section. Verify host certificate references/association, enum compatibility, date handling and version gates. |
| `XenAdmin/TabPages/PerformancePage.cs`: `LoadDataSources()`, `GetDataSourcesAction`, `GraphList`, `LoadEvents()` | Host RRD metrics, data source names/units, graph ranges and event overlays. Custom source selection, saved graph layouts and event overlays also need a separate parity check; they are not covered merely by rendering a chart. | Migration note says RRD charts pending: **Missing** from the current app. **Draft, unverified:** `performance.rs` reads member RRD updates plus `host.get_servertime` / `host.get_data_sources`; UI offers 15m/1h/24h charts. Verify endpoint, member TLS route, units, gaps and exclusion of VM/other-host series. |
| `XenAdmin/TabPages/AlertSummaryPage.cs`: `NewAlertRow()` / `FilterAlert()`; `XenAdmin/Alerts/Types/*`; `XenAdmin/Diagnostics/Checks/*`, `XenAdmin/Diagnostics/Problems/HostProblem/*`; `XenModel/XenAPI/Host_crashdump.cs` | Host alerts show title/description, severity/location/time. Host diagnostics are spread across checks (liveness, maintenance, reboot, patch/PBD/certificate conditions), not one generic tab. Crashdump metadata is informational; destroy is excluded. | Alerts/health are **Missing** from the current app. **Draft, unverified:** `alerts` filters message by host class/UUID; Health UI shows alerts and crash dumps only, not the diagnostic catalog. `crashdumps` reads host/timestamp/size; verify record fields and avoid dump contents/delete actions. |

**Out of scope:** VM properties, per-VM ballooning rows, VM-only `GenerateReadCachingBox()` and `GenerateDeviceSecurityBox()`/vTPM, and VM alerts shown on a host performance timeline. The host-level memory aggregates above remain in scope.

## Draft read path and boundary

`crates/xen-core/src/host.rs` drafts `Connection::host_details`: `host.get_by_uuid` → `host.get_record`, then host-scoped read-only record collections (`get_all_records`) for metrics, pool, CPU, PIF/network, PBD/SR, GPU/PCI/vGPU, USB, certificates, messages, patches/updates and crash dumps. It maps sections to ready/unsupported/unavailable. `crates/xen-core/src/performance.rs` drafts host-clock, data-source and RRD reads. `native/src/views/host_details.rs` defines the detail tabs.

The native client opens host details as an object tab from the Infrastructure tree. The backend and UI are integrated; live qualification on additional XCP-ng versions remains pending. Read paths must use explicit display-field allowlists and keep credentials, auth configuration, license secrets, power-on config and private device config out of output. `MESSAGE_METHOD_UNKNOWN`, permission denial and other read failures must not be shown as an empty list.

## Acceptance checklist for future Tasks 2–5

### Task 2 — Rust host-information API and tests

- [x] Reconcile General/version fields and gates with exact legacy helpers, including IQN, uptime/clock, custom fields, tags/folder, maintenance/TLS and update restart warnings.
- [x] Match CPU detail ordering/grouping and resolve the three host-memory calculations: physical, available-for-placement (`tot_dyn_min`) and Xen/Dom0; keep VM detail excluded.
- [x] Prove UUID scoping, field allowlisting and error-state distinctions with local fixtures; no production host calls.


- [x] Compare host PIF/network fields, FCoE/SR-IOV/version gates; host PBD/SR attachment and capacity values; PGPU/vGPU/PCI and USB fields.
- [x] Compare reboot/toolstack warnings, host certificate display and typed host alerts; define which host diagnostics are included in Health.
- [x] Verify host isolation, redaction, unknown-method vs denied/unavailable handling, and related-record failures with local mocks.

### Task 3 — Host detail UI

- [ ] Add a dedicated host detail route with General, CPU/memory, networking, storage, hardware, updates, certificates and health tabs.
- [ ] Scope every view by connection and host UUID; do not alter VM detail flows.
- [ ] Show loading/retry, empty, unsupported, unavailable and dependency warnings separately.

### Task 4 — Performance/RRD

- [ ] Verify host RRD sources, units, intervals/ranges, host/member routing, gaps/timestamps and filtering against VM/other-host series.
- [ ] Record remaining differences in source selection, custom graph layouts and event overlays; do not claim full graph parity from the initial range charts.

### Task 5 — Integration and evidence

- [ ] Verify the integrated detail route in `App.tsx`; draft components alone do not establish a working host-detail flow.
- [ ] Verify loading/retry, empty, unsupported, unavailable, warnings and refresh in the actual app; run the scoped build/tests.
- [ ] Separate local mock evidence from real host/version qualification. Task 1 made no live calls and did not change production `10.100.1.11`.

## Task 2 result — Rust API and local verification

Implemented `Connection::host_details(host_uuid)` and workspace/native `host_details` dispatch with both connection ID and host UUID. The Rust API covers General/software/BIOS, CPU, physical and derived memory, NIC/IP/bond/VLAN/SR-IOV/FCoE, host storage attachments and multipathing, GPU/PCI/USB, updates, certificates, host messages and crash-dump metadata. Source fields are explicitly selected. VM records are used internally only for host memory aggregates; VM names and per-VM detail rows are not exported.

Corrected and regression-tested differences found during review:

- Legacy maintenance state uses uppercase `MAINTENANCE_MODE`.
- VLANs inherit the same-host logical SR-IOV parent state; missing/foreign parents are unavailable.
- Advertised but unresolved CPU/PIF/PBD/PGPU/PUSB references are distinguished from genuine empty inventories.
- Bond, VLAN and SR-IOV metadata require consistent same-host endpoints before being exported.

Local verification on 9 November 2026:

- `cargo test --workspace --locked`: 34 passed, zero failed. This includes 25 core unit tests, 4 host-detail HTTPS integration tests and 5 workspace tests. Fifteen core unit tests target host information.
- `cargo fmt --all -- --check` and `cargo clippy --workspace --all-targets --locked -- -D warnings`: passed.
- `cargo build -p xen-native --locked`: passed.
- Local HTTPS fixtures exercise host isolation, private-field redaction, memory formulas, missing dependencies, API absence, permission failures and read-only method selection. Fixture method logs contain method names only.
- Targeted independent static review found the four findings above resolved.

Task 2 does not establish compatibility with every XCP-ng version or restore the complete legacy Diagnostics catalog. The API returns source information and explicit read/dependency warnings; richer diagnostic workflows still need their own mapping. Performance/RRD remains Task 4. Draft performance code is compiled as part of the shared crate, but no live RRD endpoint has been queried in this task.

Task 3 is now implemented in the native client as a host detail object tab and was exercised read-only against `192.168.8.170`. Full RRD graph parity and additional XCP-ng version qualification remain pending.
