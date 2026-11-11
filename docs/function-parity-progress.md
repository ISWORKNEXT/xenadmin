# Function parity progress

The current priority is functional coverage of XCP-ng Center, including menus, buttons, state restrictions, task results, and cleanup. Further visual redesign is deferred. A feature counts as qualified only for the behavior actually exercised against a real host.

The [baseline audit](audit-parity-20261111.md) remains the complete source comparison for the surfaces it inventories. Its counts describe the audit baseline, before the implementation follow-ups below. They are not a current completion percentage.

## Follow-up implementation on 2026-11-11

| Capability | Current behavior | Evidence and remaining limits |
| --- | --- | --- |
| New VM from template | Six-step creation wizard supports resources, SR, disk, guest network, ISO, and home-server selection. | [New VM qualification](new-vm-live-qualification-20261111.md). Actual wizard creation and both installation and installed-template paths passed. Multi-host placement and OS installation remain unqualified. |
| Host details from the tree | Host UUID identifies the object. Host references are resolved separately for inventory scope. | Follow-up to the routing failure in the baseline live UI audit. |
| Guest-network selection | New VIFs exclude the internal `xenapi` network and require a selection. | Follow-up to the default-network failure in the baseline live UI audit. |
| VM26: New VM from snapshot | A snapshot action creates an independent halted VM, marks cloned disks as owned, and refreshes inventory through task tracking. | Actual UI creation and independent XenAPI readback passed. This action currently asks for a name. It does not provide the upstream resource-selection wizard, so the full VM26 row remains partial. |
| VM27: New template from snapshot | A snapshot action creates a template with `other_config.instant=true`, independent disks, and task tracking. | Actual UI creation, template flag, instant flag, and independent disk UUIDs passed. The generated description still inherits the source description. |
| Action availability after template conversion | `VM.update_allowed_operations` follows a change from template to VM. | The real host initially retained template-only operations after `set_is_a_template(false)`. A follow-up readback includes Start and Snapshot, and both operations executed on a temporary VM. |
| Delete an attached VIF | The backend checks whether unplug is allowed, waits for unplug, tolerates an already-detached response, then destroys the VIF. | Real attached-VIF removal passed while the temporary Debian guest remained Running. An earlier attempt before guest PV drivers were ready was rejected. No force-unplug fallback was added. |
| VMSS minute selection | The UI offers 0, 15, 30, and 45. The backend rejects other minute values. | Minute 7 was rejected locally. A disabled, unassigned policy at minute 15 was created on the real host and removed. |
| Guest OS and PV-driver versions | VM details read the structured XenAPI version maps instead of treating those maps as strings. | [Guest readback](audit-evidence-20261111/snapshot-actions/final-guest-vif.jsonl) records Debian GNU/Linux 11 and PV drivers 7.30.0.19 from a newly booted temporary guest. |
| XVA import and export | Native actions stream XVA files, select storage and a guest network on import, track tasks, reject incomplete archives and existing export paths, and cancel local downloads. | [XVA qualification](xva-live-qualification-20261111.md). VM and template round trips, actual UI actions, export cancellation, and full raw-disk checksums passed on `.170`. OVF, OVA, and the broader import/export rows remain incomplete. |

The snapshot paths clone existing disks without provisioning a replacement root disk. They preserve the source snapshot and firmware. The template path avoids setting `is_a_template=true` when the clone is already a template, because the real host rejects that redundant setter with `VM_IS_TEMPLATE`.

The snapshot backend tests use [parity_snapshot_live.rs](../crates/xen-core/examples/parity_snapshot_live.rs). The example accepts credentials through stdin, restricts mutations to `192.168.8.170`, and creates objects under a unique `zz-parity-snapshot-` prefix. Credentials are absent from the evidence files.

Backend evidence includes [snapshot output checks](audit-evidence-20261111/snapshot-actions/final-core.jsonl), [attached-VIF and VMSS checks](audit-evidence-20261111/snapshot-actions/final-vif.jsonl), and [independent UI readback](audit-evidence-20261111/snapshot-actions/ui-readback.json). The first file also records the early PV-driver rejection. It is not an all-passing suite result.

UI evidence includes the [VM creation task](audit-live-ui-evidence-20261111/snapshot-actions/first-snapshot-vm-tasks.png), [template creation task](audit-live-ui-evidence-20261111/snapshot-actions/snapshot-template-final.png), and [template click sequence](audit-live-ui-evidence-20261111/snapshot-actions/actions.json). The first template attempt failed and cleaned up its temporary object. Its failure screenshot is retained separately.

The final [cleanup readback](audit-evidence-20261111/snapshot-actions/cleanup.json) found no test VMs or policies. Both the original VM UUID list and the entire VDI reference list matched the baseline. Format, Clippy across all targets, and the native build passed. No fixture suite was run for this follow-up.

## Completion scope

The user requested all remaining functions except permissions and certificates. Work and qualification are tracked in [the completion plan](parity-completion-plan.md) and [the decision trail](parity-decisions.tsv). New advanced VM operations are documented in [their live qualification](advanced-vm-live-qualification-20261111.md). VM resource/metadata ownership, long-description creation and common name, description and tag editing are tracked in [object-properties qualification](object-properties-live-qualification-20261111.md).

Full independent VM/template copies to a selected same-connection SR, interruption recovery, manual Reconnect and automatic session-expiry reconnect are tracked in [full-copy qualification](full-copy-live-qualification-20261111.md). Running/cross-pool copying, cancellation, another SR and larger/multiple disks remain outside that qualified path.

## Remaining functional coverage

The remaining implementation groups include VM migration and copy, OVF and OVA import and export, advanced snapshots and VM properties, storage discovery and creation, host and pool administration, network configuration, recovery services, monitoring, and organization features. The individual gaps and upstream source links remain in the baseline capability tables and command crosswalks.

The development lab currently supplies one host. The user confirmed that there is no second development host and directed work to continue on features that can be tested on `.170`. Cross-host migration, pool membership, HA failover, storage backends, and hardware-dependent behavior require suitable real targets before they can be qualified. This limitation does not mark those capabilities complete.
