# Polish backlog for M9 (collected during supervisor review)

- New Search dialog is a flat form; upstream uses a row-based query
  builder ("Search for [type] where [field] [operator] [value]" with +/-
  rows and nested groups). Port the upstream layout.
- Organization view group rows (custom field, folder, value nodes) use the
  home icon; use upstream folder / tag / custom-field icons.
- Maintenance Mode enter/exit not live-tested (single host with running
  pre-existing VMs). In M9, shut down/suspend XOA and Windows 10 briefly,
  test enter/exit, then restore their power state.
- Start in Recovery Mode returned FAILED_TO_START_EMULATOR on the lab.
- CBT disable, Quick Create, Server Status Report: live check inconclusive.
- Host clock on .170 is ~7 h ahead (local time stored as UTC).
