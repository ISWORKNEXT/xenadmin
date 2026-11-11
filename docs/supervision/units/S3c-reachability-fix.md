# Unit S3c — Restore commands lost in S3b (small fix)

Read `docs/supervision/CHARTER.md` first. It is binding. Keep this small.

## Why

S3b (uncommitted, in the working tree) removed the old host/pool page
button menus. Supervisor review found these actions are no longer emitted
by any view, so users cannot reach them:

- `UiAction::OpenPoolOptions` (Pool options dialog, including the pool-wide
  VM autostart option `other_config.auto_poweron`). Was "Pool options…" on
  the pool page and "VM autostart (pool-wide)…" in "Host actions…".
- `UiAction::OpenVlanCreate` (create VLAN network). Was in "Manage network…".

## Deliver

1. Pool menu > Properties (`PoolPropertiesCommand`) and Server menu >
   Properties for a standalone host open the Pool options dialog when the
   app has no other pool properties dialog; otherwise add the pool options
   as the dialog upstream uses. The dialog must keep the VM autostart
   option. Keep the existing `HostPropertiesCommand` behavior if it already
   opens something.
2. Host Networking tab > "Add Network…" opens a small chooser matching the
   upstream New Network wizard's first page (External Network = VLAN,
   Bonded Network, Single-Server Private Network disabled "Not implemented
   yet") that leads to the existing VLAN and bond dialogs. Also wire Pool
   menu / Server menu "New Network…" if listed in `menubar.rs`.
3. Re-check every `UiAction` variant in `native/src/state.rs`: list any
   variant that is handled in `app.rs` but emitted by no view or menu. Wire
   each to its upstream place, or report it under FOUND, NOT FIXED with the
   reason.

## Accept

- fmt, clippy, tests pass.
- Real app on the dev host, real clicks: Pool/Server Properties shows the
  autostart option (screenshot); Add Network… reaches the VLAN dialog
  (screenshot). No live mutation needed. Evidence in
  `docs/supervision/evidence/S3c/`.
