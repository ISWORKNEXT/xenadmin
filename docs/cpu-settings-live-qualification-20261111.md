# VM41: CPU topology and scheduling weight — 2026-11-11

The existing Resources editor and backend now support CPU scheduling weight (1–65535) and cores per socket. Both change only while Halted, matching the pinned upstream CPU page. Optional absent fields preserve existing values. Native inputs retain their original strings, so unrelated edits do not normalize untouched legacy keys.

Topology validates divisibility, at most 16 sockets, and the effective home host's CPU-count/socket-count limit before writes. A CPU-maximum change also validates inherited topology when no new topology is supplied. Configured creation performs an inherited-topology preflight before cloning when CPU count changes. Home selection ports upstream snapshot ancestry/template handling, running resident host, the first managed/nonmissing VDI on a local one-PBD SR (including CDs), live affinity, and maximum-pool fallback. Multi-host selection remains source-reviewed only.

Changed weight merges into a fresh `VCPUs_params` map; topology and secure-boot changes merge together into a fresh platform map. Only changed keys are inserted and touched values are read back. This is not an atomic multi-field transaction or a compare-and-swap guarantee against another writer.

## Real core and native checks on .170

[Core trace](audit-evidence-20261111/cpu-settings/core-live.jsonl) creates a uniquely owned blank VM, seeds cap/mask and a platform sentinel, then verifies maximum4/startup2, cores2 and weight512 followed by weight1024. Unrelated captured maps, name/description/tags and memory values match. Weight0, nondividing topology, and max3 with omitted topology while retaining cores2 all reject without other writes. Omitted options preserve maps; read-only changes reject. The owned blank VM is started; Running weight/topology changes reject without altering the captured Running state, then the VM stops.

Two earlier attempts failed a post-power-cycle whole-state comparison, but both cleaned all five inventories exactly. [Attempt2 diagnostics](audit-evidence-20261111/cpu-settings/attempt2-output.txt) established that starting the VM added only previously absent `platform:device-model=qemu-upstream-compat` and `timeoffset=0`. The passing harness accepts only those newly added keys observed at Running state and requires all pre-existing fields unchanged. The failed [attempt1](audit-evidence-20261111/cpu-settings/attempt1-output.txt) and attempt2 remain retained.

The [native sequence](audit-evidence-20261111/cpu-settings/native-ui/actions.json) opens Resources and edits weight2048/core1. After opening, an independent client sets cap10 and adds a platform key. [Independent Save readback](audit-evidence-20261111/cpu-settings/ui-save-independent.json) proves both requested CPU values and exact preservation of those external values and all other captured fields. Authoritative reopen shows the new settings. At 1000×720, [weight0 rejects visibly](audit-evidence-20261111/cpu-settings/native-ui/cpu-invalid-weight-settled.png) and [raw state remains unchanged](audit-evidence-20261111/cpu-settings/ui-invalid-no-write.json). Native restoration returns weight1024/core2 while retaining the external cap/platform changes, with [exact readback](audit-evidence-20261111/cpu-settings/ui-restored-independent.json).

## Actual hypervisor state

The bounded owned blank VM was started again. [Xen credit scheduler output](audit-evidence-20261111/cpu-settings/hypervisor-runtime.json) verifies actual domain21 weight1024 and cap10, beyond the XenAPI configuration record. The raw `xl vcpu-list` output is retained; this is not guest-OS CPU-topology or active-vCPU-count qualification, nor CPU-affinity qualification.

The [Running Resources screenshot](audit-evidence-20261111/cpu-settings/native-ui/cpu-running-resources-verified-settled.png) shows topology/weight inputs disabled. A no-change Save preserves [exact Running state](audit-evidence-20261111/cpu-settings/running-after-ui-noop.json). The runtime helper then [stops the owned VM](audit-evidence-20261111/cpu-settings/runtime-stopped.json).

The first runtime probe used Python's newer `subprocess(text=...)` option, unsupported by host Python3.6. [Its exact error](audit-evidence-20261111/cpu-settings/runtime-probe-python36-error.json) and [automatic owned-start rollback](audit-evidence-20261111/cpu-settings/runtime-probe-rollback.json) are retained. `universal_newlines` corrected compatibility and the retry passed. Images named `cpu-running-…` without `verified` come from that failed probe's already-Halted state and do not qualify Running behavior; only the explicitly linked verified image does.

## Cleanup and limits

After stopping the VM and exiting native writers, [raw graph admission](audit-evidence-20261111/cpu-settings/cleanup-before-resume.json) succeeded and `exit_code == 0` gated resumption. Automatic cleanup finishes with exact baseline UUID sets: 68 VMs, 81 VDIs, 5 SRs, 5 PBDs and 2 networks. Owned SR/PBD/path and guests/disks were removed.

Native build, all-target check/Clippy, formatting and Python compilation passed. Guest CPUID/socket presentation, scheduler performance under load, multi-host behavior, template/snapshot resource editing and topology selection in the New VM wizard remain outside this unit. Unrelated-key changes after opening were exercised; simultaneous same-key writers and failures between multiple setters were not.

The earlier retained-creation oversized-CPU fault probe is historical: the new topology preflight now rejects that input before cloning. Its prior real retention evidence remains valid for the earlier revision; this unit does not requalify a new post-clone failure injection.
