# New VM CPU selection qualification

The configured template/snapshot creation request now separates maximum and initial vCPUs and accepts explicit cores per socket. The native Resources and Review steps expose these settings. All existing callers migrated to explicit equal counts where that was their previous behavior; the old `CreateVmRequest.vcpus` field is removed.

[Design and upstream grounding](new-vm-cpu-design.md) compare the pinned `Page_CpuMem` behavior with the existing Resources owner. Creation rejects zero, initial above maximum and invalid topology arithmetic before cloning. Separate counts require a non-Windows source and every host explicitly advertising `restrict_set_vcpus_number_live=false`. Unsupported sources default both counts from source startup, and disable the Initial control. Unknown/missing restriction values remain restricted. Older pre-Ely edition-based licensing is not qualified.

CPU and memory writes reuse `update_vm_resources` after configuring storage, media and home server. Its physical topology check therefore sees final placement. A failure at that stage retains the known output, as the existing creation-retention policy requires. This is not an atomic creation transaction or durable recovery workflow.

## Real core and native workflow

[The real core trace](audit-evidence-20261111/new-vm-cpu/core-live.jsonl) uses a unique owned file SR and a small checksum disk on `.170`. It prepares source maximum4/startup2/core2, snapshots it, and creates a separate VM on the other SR at maximum6/startup3/core1 with384MiB RAM. The entire32MiB disk checksum matches, and captured source CPU/maps/memory/metadata remain exactly equal. Zero initial, initial7 above maximum6, core4 with maximum6, and inherited core2 with maximum3 reject before cloning; all five UUID inventories remain unchanged for each rejection.

[The actual native sequence](audit-evidence-20261111/new-vm-cpu/native-ui/actions.json) starts the snapshot wizard, pastes a Thai description, rejects maximum3/core2 with [Next disabled](audit-evidence-20261111/new-vm-cpu/native-ui/wizard-invalid-topology.png), then selects maximum4/startup1/core4,512MiB, the owned destination SR, guest network and the sole home host. [Review](audit-evidence-20261111/new-vm-cpu/native-ui/wizard-review.png) displays those exact settings. Create VM completes; [independent XenAPI and full-disk readback](audit-evidence-20261111/new-vm-cpu/native-independent-readback.json) confirms every selected value, independent output UUID and unchanged32MiB checksum. This does not qualify guest-visible socket presentation or runtime CPU hot-add.

## Windows-source guard and defaults

[The opt-in guard example](../crates/xen-core/examples/parity_creation_cpu_guards_live.rs) reads the real Windows10 builtin template and invokes creation with unequal counts. It also supplies an invalid SR, ensuring a guard regression cannot create or provision a Windows disk. [The result](audit-evidence-20261111/new-vm-cpu/windows-core-guard.jsonl) verifies the specific unsupported-count error occurs before SR validation and source details remain unchanged.

The native check clones that actual Windows10 builtin into a separately owned, unprovisioned template and seeds maximum4/startup2. No disk is provisioned and no Windows guest is started. [The real wizard](audit-evidence-20261111/new-vm-cpu/windows-native-ui/windows-defaults.png) defaults to2/2 and disables Initial. [At1000×720](audit-evidence-20261111/new-vm-cpu/windows-native-ui/windows-defaults-small.png), fields and Cancel/Next remain accessible. Cancel is exercised. [Independent selected-field readback](audit-evidence-20261111/new-vm-cpu/windows-template-after-wizard.json) matches setup. [Its cleanup](audit-evidence-20261111/new-vm-cpu/windows-template-cleanup.json) restores its complete five-set baseline and never calls VDI.destroy.

Independent review caught and corrected two issues before final native qualification: unsupported sources initially inherited the source maximum instead of startup, and the restriction guard initially accepted `0` whereas upstream permits only literal `false`. The final source/default screenshots reflect both corrections. An early audit-example compilation failed because a local snapshot binding shadowed the inventory function; qualifying the function name fixed it before that example was run.

## Cleanup and limits

All native writers and clipboard providers exited. [The complete cleanup graph](audit-evidence-20261111/new-vm-cpu/cleanup-before-resume.json) admitted only owned objects and descendants, excluded all baseline resources and checked the private SR files before the paused harness resumed. The final trace records one bounded read-only wait for snapshot VDI records to settle, then exact restoration of all five original UUID sets:68VMs,81VDIs,5SRs,5PBDs and2networks. The harness exited successfully.

Native build, all-target Clippy, formatting and Python compilation passed. Multi-host placement and restriction aggregation, older XenServer versions, guest CPUID/topology and runtime hot-add on the created guest remain unqualified. The native success is a configured snapshot path; the separate Windows-template check is a guard/defaults/cancel path, not Windows provisioning or installation.
