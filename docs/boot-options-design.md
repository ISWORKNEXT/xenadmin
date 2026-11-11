# VM auto-power-on and PV arguments

Pinned upstream BootOptionsEditPage writes `PV_args` for PV guests and `VM.other_config:auto_poweron`. VM.SetAutoPowerOn deliberately does not change the pool key; this unit must preserve that boundary. The existing Resources owner already handles CPU, memory and boot settings, so optional changed-only fields belong there.

Add optional `pv_args` and `auto_poweron` to VmResourceUpdate, with all existing callers omitting them. Native state retains the initial values and sends only edits. PV arguments are exact strings, shown only when HVM_boot_policy is empty, matching upstream IsHVM. Reject a changed PV argument on HVM and invalid text before any resource writes. Merge auto_poweron into a fresh VM other_config and read back touched fields; leave host/pool autostart configuration untouched. Existing legacy values are not normalized by unrelated edits.

The alternative was a separate boot-properties task/API. It would duplicate UUID resolution, state/connection guards and the existing Save flow. Reusing Resources keeps one writer. It does not make multi-field saves atomic or prevent same-key races; those limitations remain explicit.

The first real PV seed retains nonempty HVM_boot_params while HVM_boot_policy is empty. This exposes a current native classification mismatch: app.rs treats stale HVM params as HVM, whereas core and upstream use policy only. Capture the actual old Resources screen before correcting that condition. Qualification must use actual native Save, independent XenAPI values and unchanged host/pool map digests, then remove only the owned diskless VM. No host reboot or PV OS boot is part of this configuration check.
