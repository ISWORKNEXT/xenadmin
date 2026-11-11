# Pool-wide VM autostart qualification

[Upstream grounding](pool-autostart-design.md) follows ChangeHostAutostartAction and Host/Pool.SetVmAutostartEnabled: this flag belongs to pool.other_config and applies across the pool. It is separate from each VM's auto-start flag.

Pool options now exposes the pool flag and explains both scope and per-VM prerequisite. Host actions opens the same typed dialog through `VM autostart (pool-wide)`. Changed intent preserves untouched absent/legacy values. Existing scalar-option validation precedes all writes; changed autostart merges into a fresh pool map and receives readback. Host details displays literal-true semantics as Yes/No and automatically reloads after a successful Save on the same connection. This is not an atomic multi-field or same-key compare-and-swap transaction.

## Journal and actual core checks

[The independent helper](../tools/audit_pool_autostart.py) creates an exclusive0600 journal on `.170` before any write, capturing exact original key presence/value, unrelated pool/host/VM configuration digests, scalar options and all five UUID inventories. The [original autostart key was absent](audit-evidence-20261111/pool-autostart/journal-created.json). The helper only changes the two admitted test keys; it never reboots, powers a guest, changes VM/host settings or deletes storage.

[Actual core trace](audit-evidence-20261111/pool-autostart/core-live.jsonl) verifies an exact no-op preserving that absence, mismatched-target rejection, invalid scalar rejection before autostart mutation, read-only rejection and a real enable plus idempotent retry. Independent state shows only the flag changed; all captured VM/host configuration, unrelated pool map, three scalar options and all inventories match. The actual HostDetails model reads Yes. The core returns the original false boolean, while explicitly leaving exact absent-key restoration to the journal after native qualification.

## Native menu, Save and host refresh

[The actual Host actions menu](audit-evidence-20261111/pool-autostart/native-ui/host-actions-autostart-menu.png) opens [Pool options](audit-evidence-20261111/pool-autostart/native-ui/pool-options-settled.png). After it opens, an independent client adds the uniquely owned map sentinel. Native checks only the autostart box and saves. [Independent full captured-state equality](audit-evidence-20261111/pool-autostart/native-save-exact.json) proves the requested flag and preservation of the external key and every other captured value.

Without a manual Refresh click, [Host details shows Yes](audit-evidence-20261111/pool-autostart/native-ui/host-autostart-after-auto-refresh.png). Authoritative reopen at1000×720 shows [the checked flag and accessible controls](audit-evidence-20261111/pool-autostart/native-ui/pool-autostart-reopen-small.png). Turning it off and saving preserves [all other state exactly](audit-evidence-20261111/pool-autostart/native-off-exact.json), and [the automatically refreshed host row shows No](audit-evidence-20261111/pool-autostart/native-ui/host-autostart-disabled-refresh.png).

## Exact restoration and QA runtime limitation

All native actions and independent readbacks above completed before teardown. At final teardown, the owned private Xvfb server was found exited0 and the native app exited1 with `X connection to :98 broken`. The driver then raised ConnectionClosedError while closing its display, so the whole driver process exited1. This is not an all-green driver exit. [Exact receipts](audit-evidence-20261111/pool-autostart/qa-display-close-receipts.json) retain the error, native process cleanup, PID absence, clean Xvfb exit and subsequent restoration. The exact exit moment and reason for that server exit are INCONCLUSIVE; it does not invalidate the already-captured Save results or establish a product defect.

Restoration was therefore gated on the actual cleanup receipt and absence of the recorded native PID, rather than assuming the driver succeeded. The helper checks the latest exact test-key values again against a fresh map, restores only original test keys, preserves unrelated state, and removes its journal only after full baseline equality. [Final restoration](audit-evidence-20261111/pool-autostart/exact-restoration.json) confirms the original absent autostart key, removed owned sentinel, identical VM/host/pool digests and scalar settings, and all five original inventories:68VMs,81VDIs,5SRs,5PBDs,2networks. No existing guest was stopped.

Native build, all-target check/Clippy, existing workspace tests and Python compilation passed. A local host-field type-inference compilation error was corrected before the qualifying native build. Multi-host behavior, host reboot autostart, HA interactions, same-key races and interrupted multi-field saves remain unqualified. The CPU/VM Boot releasev0.1.12 excludes this subsequent pool unit.
