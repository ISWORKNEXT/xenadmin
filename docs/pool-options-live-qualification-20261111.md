# Pool options: live qualification

The native Pool page now offers **Pool options…** for migration compression, IGMP snooping and live-patching policy. The fields mirror upstream [PoolAdvancedEditPage](https://github.com/xcp-ng/xenadmin/blob/ba43130ffb30d05d1eb40187b8ef9f2de111468a/XenAdmin/SettingsPanels/PoolAdvancedEditPage.cs), [NetworkOptionsEditPage](https://github.com/xcp-ng/xenadmin/blob/ba43130ffb30d05d1eb40187b8ef9f2de111468a/XenAdmin/SettingsPanels/NetworkOptionsEditPage.cs) and [LivePatchingEditPage](https://github.com/xcp-ng/xenadmin/blob/ba43130ffb30d05d1eb40187b8ef9f2de111468a/XenAdmin/SettingsPanels/LivePatchingEditPage.cs).

The typed read model contains the pool UUID and three optional booleans. Missing server fields remain unavailable instead of appearing as false. Save carries the original baseline and edited values, validates every changed field before mutation, writes only changed fields that are not already desired, and reads the result back. Unrelated current values are preserved. This is not an atomic XenAPI transaction; partial errors retain the native draft and offer Reload. Pool names, descriptions, tags, credentials and permissions are outside this interface.

## Real-host checks

The [live harness](../crates/xen-core/examples/parity_pool_options_live.rs) only accepts writable `.170` credentials through stdin. It records the original options in a private rollback journal before mutations. An independent SSH client reads the raw named XenAPI fields rather than relying solely on the new read model.

The [final core trace](audit-evidence-20261111/pool-options/core-live.jsonl) verifies:

- Each of the three options changes from false to true, reads back exactly and returns to false with the other options unchanged.
- An invalid request that would clear a supported option is rejected before another requested field can change.
- A stale form preserves an unrelated second-client edit, including on retry.
- A read-only connection cannot change the options.
- Pool identity metadata remains unchanged; all original options are restored and the private journal is removed.

The initial [failed read attempt](audit-evidence-20261111/pool-options/core-attempt1.jsonl) used the C# SDK class spelling `Pool` as the RPC prefix. XenAPI's wire class is lowercase `pool`; the read returned Unsupported before any journaled mutation. The new module uses the canonical wire names and the corrected run passed.

## Native checks

[Native actions and screenshots](audit-evidence-20261111/pool-options/native-ui/actions.json) exercise all three checkboxes and Save, reopen with authoritative values, restore through the UI, Reload an unsaved draft, no-op Save and Close without saving. The 1000×720 form keeps its controls and footer visible. The live-patching checkbox correctly inverts the server's `live_patching_disabled` field.

[Independent UI readback](audit-evidence-20261111/pool-options/ui-readback.jsonl) confirms all three true values after the native save, exact false/false/false restoration, and unchanged server values after Reload/no-op Save/Close. The private UI rollback journal was removed after restoration. [Inventory cleanup](audit-evidence-20261111/pool-options/inventory-cleanup.json) matches all original VM UUID, VDI reference and network reference sets.

These checks qualify the setting controls and server policy values. Compressed cross-host migration, multicast forwarding behavior and applying an actual live patch are separate operations and were not tested here. Older server versions with absent fields are source-reviewed, not live-qualified.

## Deferred legacy callers

The same uppercase prefix occurs in legacy HA/WLB and VLAN code. A temporary adjacent correction was reverted after review identified unsafe existing HA rollback: ambiguous enable errors can reset tolerated failures to zero. Those callers need their own workflow/recovery qualification rather than an isolated spelling fix. Their files are unchanged by this unit. Authentication and certificate administration remain excluded.
