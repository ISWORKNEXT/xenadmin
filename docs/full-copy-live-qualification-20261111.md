# Full VM and template copies: live qualification

The workflow uses `Async.VM.copy`, following upstream `VMCopyAction`, rather than treating a fast clone as a full disk copy. The source baseline is `xcp-ng/xenadmin@ba43130ffb30d05d1eb40187b8ef9f2de111468a`. Permission and certificate administration remain excluded.

## Behavior and native access

VM and template details offer **Full copy…** for halted objects that advertise `copy`. The dialog collects a name, description and an explicit destination SR. The backend rechecks the source UUID, type, power state and allowed operation; validates the destination UUID, writable capability, attachment and capacity; then waits for the host copy. Result validation checks the new object type, state, disk count, device/size mapping, independent disk UUIDs and selected SR. Metadata changes use the common Properties owner. Host-inherited disk ownership markers are preserved.

The native action appears in Activity & tasks and reports the resulting UUID. The live native sequences cover both a normal VM and a template, with SR selection and successful task results. Independent readback checks the resulting type, descriptions, exact tags, disk UUIDs, SR UUID and complete disk checksums.

## Live evidence

- [Core copy trace](audit-evidence-20261111/advanced-vm/verify-copy.jsonl): halted VM and template copies, source metadata preservation, independent disks and rejected ISO destination.
- [Final recovery trace](audit-evidence-20261111/advanced-vm/verify-copy-recovery.jsonl): complete 16 MiB source/copy checksums, real connection loss, retained task identity, read-only recovery, foreign-disk safety, description conflicts and Workspace reconciliation.
- [Native readback](audit-evidence-20261111/advanced-vm/full-copy-ui-readback.jsonl): both native-created copies have independent VDIs on the selected SR and matching complete 16 MiB SHA-256 values.
- Native [template actions](audit-evidence-20261111/advanced-vm/full-copy-template-ui/actions.json) and [VM actions](audit-evidence-20261111/advanced-vm/full-copy-vm-ui/actions.json).

The opted-in [live harness](../crates/xen-core/examples/parity_vm_advanced_live.rs) accepts credentials from stdin and only mutates the authorized `.170` development host under a unique owned prefix. A marker is written only to an owned 16 MiB test VDI. Checksums read every byte through temporary read-only dom0 attachments, which are removed afterwards.

## Interrupted copies and concurrent changes

The [test relay](../tools/audit_copy_fault_proxy.py) forwards encrypted traffic without inspecting it. A read-only SSH observer identifies a new real `Async.VM.copy` task, then closes only this local relay's sockets. The retained task reference must match that observed host task. No host network settings or API responses are fabricated.

Interrupted copies retain their source, requested destination, remote task and known copy UUID. Workspace keeps their source reserved and exposes Check again. Reconnect preserves the same reservations; the live Workspace test rejects duplicate copies and disconnects before and after reconnect, then finishes the existing task without submitting another copy.

Recovery does not delete a changed destination or its disks. The test attaches a previously unattached writable test disk to the interrupted destination, confirms layout rejection leaves both objects and disk bytes intact, and removes only that test attachment. Description writes retain the expected copied baseline: an external description change is rejected and preserved, while an already-applied desired value is retry-safe. Read-only recovery retains its context without writing.

The connection tree now provides a real Reconnect action using the saved in-memory session credentials. Previously its online “Reconnect / edit” action opened a connection dialog whose submission could be rejected as an already-connected pool. The [native Reconnect sequence](audit-evidence-20261111/advanced-vm/reconnect-ui/actions.json) opens that connection menu, invokes Reconnect and returns to an online inventory. The live Workspace trace independently verifies actual replacement and retained reservations.

## Qualification limits

This lab has one writable SR, so selection of that same SR is qualified; copying to another SR is not. The test uses one 16 MiB disk, not a large or multiple-disk workload. Running copies, cross-pool copies and cancellation are not implemented by this workflow. These limits keep the broader VM18 row partial.

If the initial submission response itself is lost, there may be no remote task reference. The operation stays unresolved, warns against retrying and requires manual host-task/inventory inspection. That edge is source-reviewed, not live-qualified. Recovery context is in memory and does not survive application restart. Native Check again under a relay outage remains distinct from the explicitly tested Workspace reconnect method. Automatic session-expiry recovery is checked separately below.

Native Reconnect is qualified by the linked UI sequence. Final owned-resource cleanup is recorded separately below.

## Expired sessions

The baseline audit identified SESSION_INVALID falling outside the heartbeat reconnect cases. The heartbeat now treats it as a reconnectable session failure, using the existing consecutive-failure policy. An opted-in real-host test logs out only its own session, confirms SESSION_INVALID, then checks actual active-connection replacement and a successful heartbeat/online inventory. Its receipt is [session-expiry-live.txt](audit-evidence-20261111/advanced-vm/session-expiry-live.txt). No other host session is invalidated.

## Cleanup

The final [cleanup readback](audit-evidence-20261111/advanced-vm/full-copy-cleanup.json) matches all baseline sets: 68 VM UUIDs, 81 VDI references and 2 network references. Owned normal copies, templates, source guests and the unattached foreign test disk were removed. This comparison asserts resource identity sets, not externally managed guest power states.
