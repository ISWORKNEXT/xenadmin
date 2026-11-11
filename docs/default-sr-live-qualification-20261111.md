# DS25: Set pool default SR — 2026-11-11

The missing upstream Set as default SR action now uses the existing typed SR action/task path. Backend guards require a same-connection UUID, write-enabled connection, attached writable SR supporting VDI creation, and shared storage for a pool with multiple hosts. The canonical `pool.set_default_SR` setter is followed by authoritative readback. Re-selecting the existing default succeeds without another write. Inventory supplies current-default and eligibility flags; the native SR page shows Pool default Yes/No and a Manage storage action.

The action is direct and reversible, matching upstream. An early confirmation description was removed when review showed nondestructive SR actions do not open that dialog. The action tooltip explains that new disks use this default while existing disk locations stay unchanged.

## Real single-host qualification

[Core evidence](audit-evidence-20261111/default-sr/core-live.jsonl) saves the original pool/default-SR UUID/reference in an owned private journal before any default change. It creates a temporary file SR, then verifies:

- Detached target rejected before repair; ISO target rejected.
- Private writable SR selected as default, with independent raw pool readback and refreshed inventory flag.
- Repeated selection succeeds idempotently.
- A separately logged-in read-only connection cannot change the default back.

The [native action sequence](audit-evidence-20261111/default-sr/native-ui/actions.json) starts with the private SR already default. [Its action is disabled](audit-evidence-20261111/default-sr/native-ui/current-default-disabled.png). Actual clicks then choose Local storage, the private SR, and Local storage again. [Independent reads](audit-evidence-20261111/default-sr/ui-original-restored.json), [private selection](audit-evidence-20261111/default-sr/ui-private-selected.json), and [final original value](audit-evidence-20261111/default-sr/ui-final-original.json) confirm each selection. [The native Yes flag and completion toast](audit-evidence-20261111/default-sr/native-ui/private-set-by-ui.png) agree. [ISO storage disables the action](audit-evidence-20261111/default-sr/native-ui/iso-default-disabled.png).

## Restoration and cleanup

Original pool UUID: `366d4165-4bdd-84e8-3e5e-84542311b5c3`. Original default SR: `7a5a8cb4-7052-8de1-7473-4a8d9a36e272` (Local storage). The runner restores this through the real backend, compares exact UUID/reference state, and only then removes the private recovery journal and cleans up. Raw admission also rejects deletion of a test SR while it is still the pool default.

After the native writers exited, [independent graph admission](audit-evidence-20261111/default-sr/cleanup-before-resume.json) passed and `exit_code == 0` gated resumption. The runner's `default-sr-restored` receipt proves exact restoration before automatic admission/cleanup. Final inventory sets match the original 68 VMs, 81 VDIs, 5 SRs, 5 PBDs and 2 networks; private path/SR/PBD and journal were removed.

Native build, all-target Clippy, formatting and Python compilation passed. Multi-host/shared-storage eligibility is source-reviewed only; the available lab has one host. This unit does not qualify later VM placement or disk allocation by other clients using the new default, nor change the existing wizard's explicit storage selection.
