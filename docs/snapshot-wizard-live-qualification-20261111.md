# Configured VM creation from snapshot — 2026-11-11

Upstream `NewVMFromSnapshotCommand.cs:65` opens the normal New VM wizard with the snapshot selected. The native snapshot action now opens the existing six-step wizard: source, name/description, resources/home server, storage/media, network and review. Template and snapshot creation use one configured-creation backend. `VmCreationSource` carries an explicit Template or Snapshot UUID; inventory classification and fresh details must agree before cloning. All callers migrated to `create_configured_vm`; the former template-only entry point was removed.

## Real qualification on .170

[The core trace](audit-evidence-20261111/snapshot-wizard/core-live.jsonl) captures the original inventory and uses uniquely owned small disks plus a temporary file SR. Template creation/copy and offline storage checks also pass after the API migration. Sending a snapshot UUID with Template kind rejects before cloning.

The configured snapshot output `b604a58e-6d1b-ff06-f8c3-7c6f1dc3b478` is an independent Halted normal VM, with a new 32 MiB disk on the different writable SR and complete SHA256 equality to the source. [Independent resource readback](audit-evidence-20261111/snapshot-wizard/independent-core-resources.json) verifies both CPU fields = 2, static-max/dynamic-min/dynamic-max = 384 MiB, the exact guest network and home-host UUID. The running harness initially checked interface count and logged the requested network/host values; the independent receipt supplies actual identity verification. The source assertions now check the complete values too; that later assertion-only source extension was compile-checked.

The [actual native sequence](audit-evidence-20261111/snapshot-wizard/native-ui/actions.json) opened Create VM from snapshot, confirmed the preselected source, entered description, changed CPU/RAM, chose the private SR, network and home server, then reviewed and created at 1000×720. [Small-window review](audit-evidence-20261111/snapshot-wizard/native-ui/review-small-settled.png) keeps Create VM accessible. [The task result](audit-evidence-20261111/snapshot-wizard/native-ui/creation-final.png) and [independent output/disk readback](audit-evidence-20261111/snapshot-wizard/independent-ui-readback.json) verify:

- VM `120bf26d-f6ab-1f0c-bb6b-acd179bfa19a`, normal/Halted, both CPU fields = 3 and the three memory fields = 512 MiB.
- Exact description `UI snapshot`, guest network `30afc560-17ec-23ce-75b3-e263801787ed` and home host `f2347fdf-e320-4d3b-994b-7eb12f860c58`.
- New disk `e61b0323-b021-4212-a794-d3f1feb3ac59` on SR `aebcbd79-b66b-3c29-d5eb-65aa223ce22e`, complete 32 MiB SHA256 `3d340b5a4dd59c3dfcfbaefb475dbd2ec82902a34ee27ad31a2c6dcd057f52fb`, equal to the source.

The first UI launch pointed to the release-style filename under `target/debug`, where the local binary is named `xen-native`; correcting the binary path launched successfully. The independent readback helper first used Python `removeprefix`, unsupported by the host interpreter; it failed before logging in/mutating, was changed to slicing, and then passed. No failed attempt is counted as a pass.

## Cleanup and checks

The native app/driver exited before cleanup. An independent preflight passed and its `exit_code == 0` explicitly gated runner resumption. The new runner then invoked the raw graph admission itself: snapshots are allowed only with opt-in, every descendant/parent is admitted, leaf snapshots precede source VMs, suspend images and owner-marked CDs reject, and baseline objects/external disk references/private files are checked. [Pre-resume graph](audit-evidence-20261111/snapshot-wizard/cleanup-before-resume.json) and the automatic `cleanup-admission` record in the core trace cover six VMs and six VDIs. The final trace proves all five original UUID sets match: 68 VMs, 81 VDIs, 5 SRs, 5 PBDs, 2 networks. The private SR/PBD and directory were removed.

Workspace/all-target checks, Clippy, formatting, Python compilation and native build passed. The tested native binary is identified by its startup manifest. No fixture suite was run for this unit.

## Limits and follow-up

This qualifies a halted disk-only snapshot with one small data disk, one network and one host. It does not qualify guest boot, checkpoint-memory restoration, multiple NIC mapping, per-disk destination selection, GPU/BIOS/cloud-init pages or all upstream wizard options. These remain separate parity gaps.

The existing configured-creation failure cleanup still derives disposable disks from current nonsource attachments rather than a per-created-disk ledger. Concurrent foreign-disk attachment and interruption/reconciliation protection are NOT VERIFIED by this controlled single-writer test. This inherited gap needs its own recovery/ownership work; the hardened Full copy recovery guarantees do not apply here.
