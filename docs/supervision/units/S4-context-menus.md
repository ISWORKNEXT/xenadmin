# Unit S4 — Context menus and multi-selection

Read `docs/supervision/CHARTER.md` first. It is binding.

## Why

XCP-ng Center has a right-click menu on every tree node, search row and
tab table row, built from the same commands as the menu bar. The app has
none (or old ad-hoc ones). The tree is also single-select only, while
upstream menus and the toolbar act on multi-selections.

## Reference

- `/home/tinnerkun/Desktop/xen/upstream-xenadmin/XenAdmin/Commands/Controls/ContextMenuBuilder.cs`
  (1140 lines): one `Builder` class per selection shape. Copy the item list,
  order and separators of each builder.
- Command enable rules and disabled reasons: the command classes under
  `XenAdmin/Commands/` (`CanRun`, `GetCantRunReasonCore`).
- Our command model: `native/src/views/menubar.rs` (S1). Context menus must
  reuse it: one command id → one label, one enable rule, one dispatch.

## Deliver

1. **Multi-selection in the tree**: Ctrl+click toggles, Shift+click selects
   a range of siblings. The menu bar, toolbar and Search tab read the whole
   selection. With more than one object selected the right pane shows only
   the Search tab listing the selection (upstream rule).
2. **Context menus on tree nodes**, one per upstream builder, at least:
   SinglePool, SingleAliveStandaloneHost, SingleAliveHostInPool,
   DisconnectedHosts, SingleVM, SingleTemplate, SingleSnapshot, SingleSR,
   SingleVDI, SingleNetwork, SingleFolder, SingleTag, MultipleVMsInPool,
   MultipleTemplates, MultipleSnapshots, MultipleSRs, MultipleAliveHosts,
   MixedVMsAndTemplates, MultipleDifferentXenObjectTypes. Root node gets
   upstream's root menu (Add New Server, Connect All, Disconnect All).
3. **Disabled items** stay visible, greyed, with a hover tooltip giving the
   reason (upstream `GetCantRunReasonCore` text, or "Not implemented yet",
   or "Excluded from this port" for RBAC/certificates).
4. **Commands that act on many objects**: Start, Shut Down, Reboot,
   Suspend, Resume, Force Shut Down, Force Reboot, Delete (VM/template/
   snapshot), Export to file for multiple VMs if the backend handles one.
   Run them one after another as separate tasks; one failure does not stop
   the others. Destructive ones confirm once with the list of names.
5. **Context menus on rows** of the Search tab results and the VM Storage,
   VM Networking, Snapshots, host Networking/NICs and SR Storage tables,
   holding the same items as the buttons under that table.
6. Right-click on an unselected node selects it first (upstream behavior);
   right-click inside a multi-selection keeps the selection.
7. Keyboard: the Menu key and Shift+F10 open the context menu of the
   selection; Delete runs the selection's delete command (with confirm);
   F2 renames (opens rename for VM/host/SR/network if the app has rename).

## Out of scope

Drag and drop, Notifications (S5), new backend functions, new dialogs.

## Accept

- fmt, clippy, tests pass.
- Screenshots in `docs/supervision/evidence/S4/`: context menu on a VM,
  on the host, on an SR, on a multi-selection of two VMs (with a disabled
  item's tooltip visible in one of them).
- Real clicks on the dev host: create two `zz-parity-S4-*` VMs (no disks
  needed), multi-select them, Start (if they can start) or Delete from the
  context menu; XenAPI readback shows both affected. Clean up.
- Update `docs/supervision/menu-status.tsv` for items whose state changed.

## Supervisor additions (from S3b/S3c review)

S3a/S3b removed controls whose handlers still exist in `app.rs` but are no
longer emitted by any view. Reconnect these in this unit, in the upstream
place, because they are regressions:

- `OpenDiskResize`, `SetVbdMode`, `SetVbdBootable`: VM Storage tab >
  Properties (and the row context menu) opens a virtual disk properties
  dialog in upstream layout (`VDIPropertiesDialog`/`VdiSizeLocationPage`:
  name, description, size with resize, then per-VM device options: mode
  read-only/read-write, bootable, position). Reuse the existing actions.
- `SetPifPlugged`: NICs tab row context menu / buttons where upstream has
  them; if upstream has no plug control, report it.
- `LoadPbdCatalog`, `SetPbdPlugged`: SR context menu "Detach"/"Reattach"
  per upstream `DetachSRCommand`/`ReattachSRCommand` if they map; else report.
- `DismissAlert`: leave for S5 (Notifications).
