# Advanced VM operations: live qualification

Target: the explicitly authorized development host `192.168.8.170`. This follow-up starts from release `v0.1.5`. It uses real temporary Debian 11 guests and the native application, without a simulated host. Permissions and certificate work are excluded from the completion scope.

## Verified behavior

| Operation | Observed result | Evidence |
| --- | --- | --- |
| Start paused | Core and native UI submit a paused start; independent XenAPI readback returned Paused. | [Core trace](audit-evidence-20261111/advanced-vm/actions-live.jsonl), [native task](audit-evidence-20261111/advanced-vm/ui/paused-result.png) |
| Checkpoint | Core and native UI create a Suspended snapshot with a separate 1,459,617,792-byte memory VDI. The source returns to Running. | [Core trace](audit-evidence-20261111/advanced-vm/actions-live.jsonl), [UI memory readback](audit-evidence-20261111/advanced-vm/ui-readback.jsonl) |
| Convert to template | Core and UI convert a Halted VM in place. UUID and attached disk UUIDs are preserved. A running VM is rejected. Retrying the core operation succeeds without copying disks. | [Core trace](audit-evidence-20261111/advanced-vm/actions-live.jsonl), [UI readback](audit-evidence-20261111/advanced-vm/ui-readback.jsonl) |

The core trace is produced by [the live harness](../crates/xen-core/examples/parity_vm_advanced_live.rs). The native run uses actual X11 input events, screenshots and independent read-only XenAPI queries over SSH. No credentials are written to the evidence.

## Limits and follow-up checks

Quiesced snapshots are implemented through the host's `snapshot_with_quiesce` operation. The tested Linux guest does not advertise that operation. Its control stays disabled and the backend rejects it; a successful quiesced snapshot remains **NOT VERIFIED**. There is no fallback to a disk-only snapshot.

The initial native screenshots captured delayed frames. Waiting for subsequent frames confirmed that the action modal opens correctly. The test driver's rapid keyboard remapping also corrupted the checkpoint name; the persisted name is recorded exactly in the independent readback. This run qualifies checkpoint creation and memory capture, not exact text entry through that driver.

The native run exposed a stale details panel after a power action: the inventory showed Running while cached details still showed Halted. The implementation now refreshes details when reopening a VM tab and when its active task completes. The updated native build successfully added a CPU to a Linux guest with Viridian enabled, and refreshed the resource display. The focused fast-task completion edge remains a source-reviewed guard.

Live CPU and disk-growth qualification continues in the `verify-resources` trace. The first two attempts retained their failures and cleaned up. A failed disk check is not reported as a successful growth operation. API selection follows upstream `VDISizeLocationPage.SaveSettings`: prefer `resize` when allowed, otherwise use `resize_online` when allowed. The dev host's attached ext-backed disks currently advertise neither operation, so successful online growth remains **NOT VERIFIED** on this lab.

Host-selected paused start passed on the single dev host: the native confirmation names the target, independent readback identifies its host UUID, and the refreshed details panel shows Paused. See [the final UI evidence](audit-evidence-20261111/advanced-vm/final-ui/paused-details-refreshed.png) and [readback](audit-evidence-20261111/advanced-vm/resource-ui-readback.jsonl). Whole-product regression checks remain pending. This document does not mark the complete parity plan finished.

## Live CPU addition

The [final core trace](audit-evidence-20261111/advanced-vm/verify-resources.jsonl) reads back two online vCPUs with a maximum of four while Running. It also rejects a live decrease and a maximum change. The native UI then raised that guest to three vCPUs, independently confirmed in [the resource readback](audit-evidence-20261111/advanced-vm/resource-ui-readback.jsonl).

An owned-guest setup subsequently enabled Viridian and reset startup CPUs to one while Halted. The final native build recognized the running Debian guest from guest metrics and successfully added a second vCPU. This checks the upstream Linux override before the HVM/Viridian Windows fallback. A real Windows-guest rejection has not been exercised.

The Running disk menu now keeps actions within the table even with a long disk name. Resize stays disabled when the VDI advertises neither resize operation. Choosing Move dismisses the popup; this lab has no second writable SR, so opening its destination dialog remains unqualified.

The API selection reference is upstream [VDISizeLocationPage.SaveSettings](https://github.com/xcp-ng/xenadmin/blob/ba43130ffb30d05d1eb40187b8ef9f2de111468a/XenAdmin/SettingsPanels/VDISizeLocationPage.cs#L101). Independent paused-state, Running-after-checkpoint and attached-disk capability observations are preserved as [historical command receipts](audit-evidence-20261111/advanced-vm/historical-readback-receipts.jsonl).

## Resource editor integration

The Resources dialog now sends only CPU, memory and boot settings. Native saves changed halted startup CPUs 2→3→2 and running CPUs 2→3 on the owned long-description guest. External name, description and tag edits made while Resources was open survived the successful save. The native boot-order control changed order to cd. [Actions and screenshots](audit-evidence-20261111/advanced-vm/resource-owner-ui/actions.json), [metadata hashes](audit-evidence-20261111/advanced-vm/property-ui-readback.jsonl), and [boot readback](audit-evidence-20261111/advanced-vm/boot-ui-readback.json) record the results. Early automation clicks used the modal's initial coordinates before recentering, edited the wrong draft field and never submitted; the retained sequence shows the correction before the actual successful saves.

[Provisioning and resource preservation checks](audit-evidence-20261111/advanced-vm/verify-provisioning.jsonl) verify omitted boot options, absent secureboot and unrelated map-key preservation. Final cleanup removed every owned guest, snapshot, template and added disk/network: [VM UUID, VDI reference and network reference sets](audit-evidence-20261111/advanced-vm/cleanup.json) exactly match the baseline. This comparison does not assert that externally managed guest power states stayed unchanged.
