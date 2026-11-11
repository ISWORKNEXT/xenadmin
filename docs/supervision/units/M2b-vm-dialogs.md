# Unit M2b — VM dialogs: snapshot, delete, copy/move, snapshot tree

Read `docs/supervision/CHARTER.md` first. It is binding.

## Reference (upstream, `/home/tinnerkun/Desktop/xen/upstream-xenadmin/XenAdmin/`)

- `Dialogs/VmSnapshotDialog.cs` (+ resx): Take Snapshot dialog.
- `Dialogs/ConfirmVMDeleteDialog.cs` (+ resx): Delete VM dialog.
- `Dialogs/CopyVMDialog.cs` / `Commands/CopyVMCommand.cs`,
  `Commands/MoveVMCommand.cs`, `Wizards/CrossPoolMigrateWizard/`
  (intra-pool copy/move pages only; cross-pool is untested-lab).
- `Controls/SnapshotTreeView.cs` and `TabPages/SnapshotsPage.cs`
  (tree view vs list view toggle, properties panel on the right).

## Deliver

1. **Take Snapshot dialog** (`VmSnapshotDialog`): name (default "<VM>
   snapshot <date>"), description, the three modes as radio buttons with
   their upstream help text: disk-only, quiesced (disabled with upstream
   reason if VSS/tools absent), disk and memory (checkpoint; only when
   running). Wire to the existing snapshot backend; checkpoint if the
   backend supports `VM.checkpoint`, else disabled "Not implemented yet".
2. **Delete VM dialog** (`ConfirmVMDeleteDialog`): list of the VM's disks
   with checkboxes (default: checked for disks owned only by this VM,
   unchecked for shared/ISO), list of snapshots with checkboxes, Select
   All / Clear All for each, Delete / Cancel. Works for a multi-selection
   (one dialog listing per VM as upstream). Replace the current confirm.
3. **Copy VM dialog** (`CopyVMDialog`): name, description, Fast clone vs
   Full copy radio buttons, target SR list for full copy with free space.
   Reuse the existing `vm_copy.rs` backend paths; restyle to upstream.
   **Move VM**: intra-pool move of a halted VM's disks to another SR,
   per `MoveVMCommand` (reuse existing move-disk path per VDI). Label
   cross-pool migrate `untested-lab` and port the wizard page structure
   only if the backend exists; otherwise keep it disabled.
4. **Snapshots tab**: add the upstream view toggle (Tree view / List
   view). Tree view draws snapshots as a tree with "Now" marker (parent
   links via `VM.parent` / `snapshot_of`), selection shows the right-hand
   properties panel (name, description, created, tags, folder, custom
   fields, memory/disks). List view keeps the current table.
5. **Polish from M2a review** (Properties dialog):
   - Vertical tab rows ~55 px: make them 38 px (title + subtitle tight).
   - Use type icons per page as upstream (general, custom fields, cpu,
     boot, ha, alerts, home server, gpu, usb, advanced) instead of ◇/●.
   - SubText must be the real summary like upstream (e.g. Alerts "None
     configured", Home Server "Unassigned"/host name, Log Destination
     "Local"), never the generic word "Configuration".
   - Boot Options: show boot order as upstream's ordered list (Hard Disk,
     DVD-Drive, Network) with Move Up / Move Down and checkboxes, not the
     raw "dc" string; firmware as "BIOS"/"UEFI"; after-crash as upstream's
     display text.
   - Default dialog size 760×560 so OK is never clipped; title text 15 px.
   - Storage tab: selecting a row must work from any cell, including Name.

## Out of scope

vApp and snapshot schedule management (M8), install guest tools (later
M2c), cross-pool migrate testing.

## Accept

- fmt, clippy, tests pass.
- Screenshots in `docs/supervision/evidence/M2b/`: Take Snapshot dialog,
  Delete VM dialog with disks and snapshots, Copy VM dialog, Snapshots tab
  tree view, Properties Boot Options page.
- Real clicks on a `zz-parity-M2b-*` VM with one 1 GiB disk: take a
  snapshot via the dialog, full-copy the VM to the same SR, then Delete
  both VMs choosing to delete disks and snapshots; XenAPI readback shows
  no leftover VDIs from them. Never touch the existing templates.
