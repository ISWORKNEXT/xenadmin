# Storage actions live qualification — 2026-11-11

Target: the authorized single-host `.170` development pool. This follow-up uses a temporary file SR under a uniquely owned `/var/tmp/zz-parity-storage-…/sr` directory. It does not qualify another physical host, shared storage, or another storage driver.

## Verified behavior

[Core receipts](audit-evidence-20261111/storage-actions/core-live.jsonl) record the original UUID inventory, file-SR creation, and these real operations:

- Full VM copy from Local storage to the temporary file SR, with independent disk UUID and complete 16 MiB SHA256 equality.
- Convert that copy to a template, then copy the template back to Local storage with independent disk UUID and complete checksum equality.
- Move the source data disk in both directions, preserving its VBD properties and contents. Each move produces a new VDI UUID.
- Grow the halted source disk from 16 to 32 MiB. The original 16 MiB checksum is unchanged and the added tail reads as zero.
- Detach the private SR PBD, repair the attachment and scan the SR.

[Published Linux v0.1.7](audit-evidence-20261111/storage-actions/native-ui/startup.json) then performed Full copy through the actual dialog, selecting the private destination SR. [The independent disk readback](audit-evidence-20261111/storage-actions/published-ui-disk-readback.json) records VM `67962684-4666-5a9e-68ef-f8aeb51c7170`, new VDI `df5a1ce5-3733-4593-b91e-b87c372d119c`, destination SR `ffbd70d9-9d28-bcd9-cb5d-19ec2b91fbe0` and complete 32 MiB SHA256 `3d340b5a4dd59c3dfcfbaefb475dbd2ec82902a34ee27ad31a2c6dcd057f52fb`, equal to the source. [The settled destination selection](audit-evidence-20261111/storage-actions/native-ui/unicode-real-clipboard.png) and [result](audit-evidence-20261111/storage-actions/native-ui/cross-sr-copy-final.png) accompany the [actual action sequence](audit-evidence-20261111/storage-actions/native-ui/actions.json).

The clipboard experiment did not replace the selected name. The running UI driver had loaded before the new paste handler, and a subsequent real clipboard attempt also left the original text selected. The copy therefore used the visible default owned name ending in `source full copy`. Unicode paste is NOT VERIFIED; the unqualified clipboard helper and handler were removed. Screenshot filenames preserve the attempted experiment, not a successful Unicode result.

## Cleanup boundary

The running core harness was already paused before cleanup source hardening. Its older binary was resumed only after the native app/driver had exited and a separate [raw deletion-graph admission](audit-evidence-20261111/storage-actions/cleanup-admission-old-runner.json) approved exactly four owned VMs, four new disks and the private SR/PBD. That read-only admission checks baseline identity exclusion, every data VBD, nonempty CDs, suspend images, snapshot descendants, current operations, external VBD references and private directory files. The actual graph had no snapshots, suspend images or nonempty CDs.

The current [harness source](../crates/xen-core/examples/parity_storage_live.rs) calls the [same admission helper](../tools/audit_storage_cleanup_graph.py) itself and disables snapshot cascade. Those corrected automatic guards compiled, but were not present in the already-running binary. The separate admission receipt is the proof for this run; it does not prove a live rerun of the newer automatic cleanup path.

The final core receipt confirms exact equality of all original UUID sets: 68 VMs, 81 VDIs, 5 SRs, 5 PBDs and 2 networks. Owned test VMs, disks, SR/PBD and private directory were removed. Pre-existing guest power states were not changed by this test.

## Limits

These are small, single-data-disk, halted objects without a guest OS. Large or multiple disks, running disk moves, attached online growth, shared-storage backends and multi-host behavior remain NOT VERIFIED. Native Move/Resize/Repair clicks were not exercised by this unit; only native cross-SR Full copy was. This follow-up closes the former different-writable-SR qualification gap for the paths described above, not the full storage parity workstream.
