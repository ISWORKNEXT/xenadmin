# VM44: auto-power-on and PV OS parameters

Resources now edits VM auto-power-on and, for PV guests, exact OS parameters. [The design](boot-options-design.md) follows pinned BootOptionsEditPage and VM.SetAutoPowerOn: the flag changes VM.other_config only and does not enable host/pool autostart. The UI explains that prerequisite. Optional fields represent changed intent; unrelated saves preserve existing values.

PV/HVM classification now uses only HVM_boot_policy, matching both core and upstream. Nonempty legacy HVM_boot_params on a PV VM no longer force HVM controls. Changed PV arguments on HVM, NUL text and more than32767 characters reject before any resource writes. Auto-power-on merges into a fresh VM map and both changed fields receive authoritative readback. Same-key races and partial multi-field failure are not atomic or reconciled transactions.

## Actual failing workflow before the fix

[The owned-object helper](../tools/audit_boot_object.py) clones Other install media into one unique diskless halted VM, clears its boot policy while retaining its HVM parameter map and seeds PV arguments. The old native Resources page displays HVM boot controls. [The actual Save error](audit-evidence-20261111/boot-options/before-ui/pv-before-actual-save-error.png) says those options are only available for HVM guests; [independent readback](audit-evidence-20261111/boot-options/actual-rejected-save-no-write.json) matches every captured VM field and host/pool map digest.

The first attempted click at y813 opened the Boot policy menu after the modal layout settled. Its misleading screenshot name `pv-save-before-rejected` is retained in the old action sequence, but it does not prove a Save attempt. The later y871 click is the actual failing Save. [The early click readback](audit-evidence-20261111/boot-options/early-click-no-write.json) is retained separately. Product changes started only after the actual failure was observed.

## Core and native verification on .170

[Core trace](audit-evidence-20261111/boot-options/core-live.jsonl) verifies exact PV arguments with preserved double spaces and auto-power-on=true, changing only those requested fields. NUL, overlong text, changed PV arguments on a temporarily HVM-classified owned VM and a read-only connection reject without other writes. Omitted fields preserve a seeded legacy disabled flag exactly. Host and pool other_config digests match the original throughout.

The corrected native page displays the existing PV parameters and omits HVM controls. [A real no-change Save succeeds](audit-evidence-20261111/boot-options/pv-native-ui/pv-noop-save-green.png); [the entire captured record remains equal](audit-evidence-20261111/boot-options/native-noop-exact.json), including the legacy flag and retained HVM parameters.

After authoritative reopen, an independent client adds a VM.other_config sentinel. Native edits PV parameters to `console=hvc0 root=/dev/xvda1  loglevel=4` and checks auto-start, then clicks Save. [Independent full captured-field equality](audit-evidence-20261111/boot-options/native-save-exact.json) proves exact arguments, true flag and preservation of that external key and all unrelated values. Reopen shows the server values. At1000×720, scrolling exposes [both controls and Save](audit-evidence-20261111/boot-options/pv-native-ui/pv-small-bottom.png). Turning off only the checkbox and saving [preserves the exact PV argument spacing and other fields](audit-evidence-20261111/boot-options/native-small-flag-off-exact.json).

The helper then classifies only this same diskless fixture as HVM. The native page hides PV parameters, exposes HVM boot options and [saves auto-start alone](audit-evidence-20261111/boot-options/hvm-native-ui/hvm-auto-draft.png). [Exact independent readback](audit-evidence-20261111/boot-options/hvm-native-save-exact.json) confirms the hidden PV arguments and all unrelated captured fields remain unchanged. [Authoritative reopen](audit-evidence-20261111/boot-options/hvm-native-ui/hvm-authoritative-reopen.png) shows the enabled flag. This fixture classification is not qualification of guest conversion or boot.

## Cleanup and limits

After all native writers and clipboard providers exited, the helper admitted the exact known UUID, unique name, halted state, no operations/snapshots/suspend image and no data VDI. [Cleanup](audit-evidence-20261111/boot-options/cleanup.json) restores all five original UUID sets:68VMs,81VDIs,5SRs,5PBDs and2networks. Host/pool map digests remain unchanged. The helper never calls VDI.destroy and no pre-existing guest was stopped.

Native build, all-target check/Clippy, formatting and Python compilation passed. The existing local workspace suite also passed115 tests with one opt-in live-session test ignored; [its exact output](audit-evidence-20261111/boot-options/local-workspace-tests.json) is retained. That unit-test run is separate from the actual native/host qualification above and does not establish hosted CI success. PV OS boot, host-start autostart after reboot, HA interactions, Running-state edits, guest conversion and interrupted multi-field Save remain unqualified. This unit qualifies configuration on the owned halted VM. Permissions and certificates remain outside scope.
