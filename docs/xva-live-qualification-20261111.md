# XVA import and export live qualification

XVA export and import were implemented and exercised against the authorized development host `192.168.8.170` on 2026-11-11. The tests used real VM, VDI, VBD, VIF, HTTP transfer, and XenAPI task operations. No mock host or fixture suite was used.

## Implemented behavior

VM, template, and snapshot details have an **Export XVA…** action. Export requires a halted object with the host's `export` operation allowed. The destination is a path on the computer running Xen Desktop. Existing files are rejected. Downloads stream into a private temporary file, and a completed file is published without overwriting an existing path.

The VM and template lists have an **Import XVA…** action. The form selects a local XVA file, a new name, an attached storage repository, and either one guest network or no networking. Imported interfaces are replaced by that selection. The imported VM or template remains halted.

Both actions run through Activity & tasks. They retain a connection lease, report host progress, and support local cancellation in addition to requesting host cancellation. This permits an export download to stop even if the host has finished producing its data. A successful import verifies that the returned object is new and belongs to the import task before changing its configuration.

The transport uses the connection's TLS and proxy settings. Session-bearing URLs remain inside the Rust transport, and redirects are not followed automatically. Uploads use HTTP PUT to `/import`; downloads use GET to `/export`. These paths follow the pinned upstream [ExportVmAction](https://github.com/xcp-ng/xenadmin/blob/ba43130ffb30d05d1eb40187b8ef9f2de111468a/XenModel/Actions/VM/ExportVmAction.cs) and [ImportVmAction](https://github.com/xcp-ng/xenadmin/blob/ba43130ffb30d05d1eb40187b8ef9f2de111468a/XenModel/Actions/VM/ImportVmAction.cs).

Before upload, the client checks the XVA metadata entry, every tar header checksum, entry bounds, and the archive footer. It does not load the archive into memory. The host checks the disk-block checksums during import.

## Observed results

| Check | Result |
| --- | --- |
| Core export | Exported a real halted VM to a 3,172,864-byte XVA file. |
| VM round trip | Imported into Local storage with a separate disk UUID, owner VBD, 1 vCPU, 256 MiB RAM, and one VIF on Pool-wide network 0. |
| Full raw-disk comparison | The source and imported disks both hashed to `18922f71ac3c9670033d57eae14f38155cd787037b59f85c9b22eea85990e6dc` across all 16,777,216 bytes. The source hash remained unchanged. |
| Existing destination | A second export to the same path was rejected. The existing archive remained usable for import. |
| Truncated archive | The completed client rejected the file before HTTP upload. |
| Corrupted disk block | A byte was changed in a real exported disk chunk while keeping the tar structure complete. The host rejected the import with `IMPORT_ERROR`. |
| Template round trip | Exported and imported a template with a mapped VIF. Its template flag and full raw-disk checksum were preserved. |
| Actual UI round trip | Clicked Export, entered a destination, clicked Import, selected SR and network, and submitted. Both tasks succeeded. Independent XenAPI and raw-disk readback matched the selections and source checksum. |
| Actual UI export cancellation | A local CONNECT proxy slowed only the test app's traffic to the real host. Clicked **Cancel task** during an actual download. The task became Cancelled, the final file was absent, and no `.part` file remained. The source disk hash remained unchanged. |

The disk checks temporarily attached only the uniquely named test VDIs to the development host's control domain. Each attachment was removed after reading or writing the test data. No existing VM disk was used for the write test.

The first truncated-file test exposed a host that kept waiting for more archive data. That test client was interrupted and its owned task was cancelled. The implementation then gained full tar-structure validation. The first rejected export also exposed an unused pending client task that XAPI refused to destroy. The client now finishes its own unsubmitted task as failed before removing it. These initial failures are retained as diagnostic evidence and are not counted as passing runs.

## Evidence

- [Final core run](audit-evidence-20261111/xva/final-qualified.jsonl) covers VM and template round trips, overwrite rejection, truncated-file rejection, corrupted-block rejection, and test-object cleanup.
- [UI raw-disk readback](audit-evidence-20261111/xva/ui-disk-readback.json) records the imported disk checksum independently of the UI task result.
- [Export and import tasks](audit-live-ui-evidence-20261111/xva/import-final.png) and the [click sequence](audit-live-ui-evidence-20261111/xva/actions.json) record actual native UI actions.
- [Cancellation screenshot](audit-live-ui-evidence-20261111/xva/cancel/cancel-requested.png), [cancel clicks](audit-live-ui-evidence-20261111/xva/cancel/actions.json), and [file and source readback](audit-evidence-20261111/xva/cancel-readback.json) record export cancellation.
- [Final cleanup](audit-evidence-20261111/xva/cleanup.json) found no test VMs or XVA tasks. The VM UUID list, VDI reference list, and network reference list exactly matched the baseline taken before qualification.

The native build, Clippy across all workspace targets, format check, and diff whitespace check passed. No fixture tests were run.

The opt-in [live example](../crates/xen-core/examples/parity_xva_live.rs) restricts mutations to the authorized development host and accepts credentials through stdin. Its cleanup mode accepts a complete generated qualification prefix. Test archives are temporary files rather than delivered VM backups.

## Qualification limits

This change implements the XVA workflow for one VM or template with zero or one mapped network interface. It does not implement OVF or OVA appliance import, disk-image import, multiple NIC mappings, UUID-preserving disaster-recovery restore, import-and-auto-start, or live export of a running VM. Cross-host storage routing and redirects remain unqualified in the one-host lab.

Export cancellation was clicked and verified. Import cancellation during an interrupted upload remains unqualified. If a network failure prevents the host task's outcome from being confirmed, the client reports that uncertainty and retains the task for inspection rather than claiming rollback succeeded. The client checks archive structure before upload; standalone export does not perform the upstream optional full block-checksum verification pass.
