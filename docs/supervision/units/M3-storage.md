# Unit M3 — Storage: New SR wizard, virtual disks, SR commands

Read `docs/supervision/CHARTER.md` first. It is binding.

## Reference (upstream `/home/tinnerkun/Desktop/xen/upstream-xenadmin/XenAdmin/`)

- `Wizards/NewSRWizard.cs`, `Wizards/NewSRWizard_Pages/` (ChooseSrTypePage,
  NewSrWizardNamePage, ChooseSrProvisioningPage, Frontends/: VHDoNFS,
  LVMoISCSI, LVMoHBA, LVMoFCoE, CIFSFrontend, NFS_ISO, CIFS_ISO,
  LocalStorageFrontend).
- Commands: NewSR, Repair, Detach, Reattach, Forget, Destroy,
  SetAsDefault, Trim, AddVirtualDisk, AttachVirtualDisk,
  DeleteVirtualDisk, DetachVirtualDisk, MoveVirtualDisk,
  MigrateVirtualDisk (`Commands/*.cs`).
- `Dialogs/NewDiskDialog.cs`, `Dialogs/AttachDiskDialog.cs`,
  `Dialogs/RepairSRDialog.cs`, `Dialogs/MoveVirtualDiskDialog.cs`.

Backend that already exists: `create_sr`, `introduce_sr`, `probe_sr`,
`reattach_sr`, `sr_action`, `repair_sr`, `pbd_catalog`/`set_pbd_plugged`,
`attach_vdi`, `delete_vdi`, `sr_vdis`, `vdi_catalog`, move-disk.
Adding backend is allowed in this unit.

## Deliver

1. **New SR wizard** in upstream page order: Type (Virtual disk storage:
   NFS, iSCSI, Hardware HBA, SMB/CIFS, Local (ext/lvm on a host disk);
   ISO library: NFS ISO, Windows File Sharing (SMB/CIFS) ISO), Name,
   Provisioning (thin/thick where upstream offers it), Location (per type
   frontend fields, Scan/Probe buttons as upstream), Finish. Uses
   `probe_sr` / `SR.probe_ext` and `SR.create` / `SR.introduce` +
   PBD create/plug for reattach of existing SRs found by probe.
   NFS/iSCSI/HBA/SMB are `untested-lab` unless the lab has a target; test
   the paths you can (Local on a spare host disk only if one is unused —
   check `PBD`/`lsblk` first and never touch a disk in use; NFS ISO or
   SMB ISO against a share you start yourself on this workstation with
   podman, e.g. an NFS or Samba container, then remove it).
2. **Virtual disks**:
   - SR Storage tab "Add…" creates a standalone VDI (`VDI.create`) with
     upstream NewDiskDialog fields (name, description, size, SR). This
     was disabled in S3b for lack of backend — add it.
   - VM Storage tab Add… uses the same dialog (create + attach).
   - Attach Disk… (AttachDiskDialog: tree of SRs → VDIs, read-only
     checkbox), Detach, Delete (with upstream confirmations), Move…
     (MoveVirtualDiskDialog), Properties (from M2a).
   - Objects view "Virtual Disks" group populated; VDI node selectable
     (General + Search tabs from S3b).
3. **SR commands** with upstream enable rules and confirm texts: Repair
   (RepairSRDialog listing PBDs per host with status), Detach (guard:
   no running VMs using it — fix the missing guard noted in S4), Reattach,
   Forget, Destroy, Set as Default, Reclaim freed space (Trim), Rescan.
4. Update `docs/supervision/menu-status.tsv`.

## Accept

- fmt, clippy, tests.
- Screenshots in `docs/supervision/evidence/M3/`: wizard type page, a
  frontend page, Attach Disk dialog, Repair SR dialog.
- Live: SR Storage Add… creates a 1 GiB `zz-parity-M3-*` VDI and Delete
  removes it (XenAPI readback); create an ISO SR from a share you serve
  yourself, Rescan shows an ISO you put there, then Detach + Forget it;
  Set as Default to another SR and back to the original default.
  Never destroy or forget a pre-existing SR.
