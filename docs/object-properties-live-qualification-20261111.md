# Common object properties: live qualification

The editor covers names, descriptions and tags for VM, template, snapshot, VDI, SR, network, host and pool objects. It uses a closed object-kind enum and selected-connection UUID resolution. Raw records and arbitrary methods cannot be submitted through this interface. Existing read-only connection guards remain enforced. Permission and certificate administration are excluded.

## Core checks

The [real-host trace](audit-evidence-20261111/advanced-vm/properties-live.jsonl) contains eight object cases: a normal VM, its disk, a temporary isolated network, the dev SR, host, pool, a temporary snapshot and a template created from that snapshot. Each case changes metadata, reads it back, rejects an invalid request without mutation, preserves an unrelated concurrent rename, rejects a conflicting same-field update and restores the original fields. The source VM retains its UUID, disks, name and power state. Temporary network/snapshot/template objects and the successful run's private rollback journal were removed.

The [live harness](../crates/xen-core/examples/parity_properties_live.rs) only accepts the writable `.170` development host and an owned halted qualification VM. Before changing an existing object's fields, it checks that its original values can be restored through the editor and writes those values to a private local rollback journal. Credentials are read from stdin and are absent from the evidence.

The first run passed five cases, then encountered the legitimate empty name of a standalone pool. The initial validator rejected restoration of that empty name. The exact original name, description and tags were immediately restored using the private journal and independently checked; see the [failed attempt](audit-evidence-20261111/advanced-vm/properties-live-attempt1.jsonl) and [recovery receipt](audit-evidence-20261111/advanced-vm/properties-pool-restore.json). The corrected run passed all eight cases, including restoration of the unnamed pool.

The form carries its original baseline alongside edits. Only user-changed fields are written. Before the first write, all changed fields are validated and compared with fresh server values. Values already matching the requested result are skipped on retries. These are optimistic conflict checks, not an atomic XenAPI transaction; an external writer can still race individual API calls. Partial-save errors retain the draft and offer Reload.

## Native checks

An owned real VM was configured with a 60-line description and 31 tags, including a tag with leading and trailing spaces. The [native screenshots and actions](audit-evidence-20261111/advanced-vm/property-ui/actions.json) show a bounded scrolling form with visible Save and Close controls. Name and description edits saved successfully. An unfinished new-tag draft was included when Save was pressed; explicit Add tag and Remove controls were also exercised.

The [independent readback](audit-evidence-20261111/advanced-vm/property-ui-readback.jsonl) checks description and tag hashes, the original tag subset and the exact spaced tag. It confirms that unrelated tags survived name/description edits and both tag workflows. These UI mutations target a temporary VM only. Backend qualification of the other object classes does not imply that every native entry point has been exercised yet.

Native conflict rejection, Reload and a no-op Save passed at a 1000×720 window size; see the [integration screenshots and action sequence](audit-evidence-20261111/advanced-vm/integration-ui/actions.json). The initial resize attempt lost its virtual display and is retained separately; the noreset-display retry passed. Non-VM native entry points remain unqualified. The complete functional parity plan remains open.

The Resources editor now owns CPU, memory and boot fields only. Its state, command and public API cannot carry names, descriptions or tags. Native CPU edits succeeded with the long description and exact tags unchanged, including metadata changed externally while the Resources form was open. See [the ownership action sequence](audit-evidence-20261111/advanced-vm/resource-owner-ui/actions.json) and the independent metadata hashes above. Shared changed-description validation also permits creation with a 2,000-character description and inheritance of a 40,000-character legacy template description, while rejecting a changed oversized description before cloning. [The provisioning trace](audit-evidence-20261111/advanced-vm/verify-provisioning.jsonl) records these cases and cleanup.
