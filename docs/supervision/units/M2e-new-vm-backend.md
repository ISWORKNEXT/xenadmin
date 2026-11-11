# Unit M2e — New VM wizard: full creation request

Read `docs/supervision/CHARTER.md` first. It is binding. Backend changes
are allowed and expected in this unit.

## Why

M2d gave the New VM wizard the upstream frame and page order, but the
creation request behind it (`crates/xen-core/src/provisioning.rs`) only
supports one disk, one VIF and ISO media. Upstream supports more. M2d
reported these gaps; this unit closes them.

## Reference

`/home/tinnerkun/Desktop/xen/upstream-xenadmin/XenAdmin/Wizards/NewVMWizard/`
pages and `XenModel/Actions/VM/CreateVMAction.cs` (the order of XenAPI
calls: clone/copy template, provision, BIOS strings, CD/boot, VIFs,
disks, affinity, start).

## Deliver

1. Creation request supports: **multiple disks** (add / edit size, SR,
   name / delete, including deleting or resizing the template's
   provisioned disks before `VM.provision` by editing `other_config.disks`,
   and for installed-OS templates resizing/moving the cloned disks);
   **multiple VIFs** (network, MAC auto/manual); **installation media**
   ISO library, physical DVD drive of the home server, network boot (PXE
   boot order), and **boot parameters** for PV templates; **Copy host BIOS
   strings** page when the template needs it (`bios_strings` from the
   chosen host, upstream `Page_CopyBiosStrings`); **home server**
   affinity; start automatically.
2. Wizard pages wire these with upstream enable rules (e.g. physical DVD
   only if the host has one; network boot hides ISO choice).
3. Guest Setup (M2c) stays and must still work: it appears for vm-data
   templates and the installed-OS path does not call `VM.provision`.
4. Unit tests for the request builder: disk list edits to
   `other_config.disks` XML, VIF ordering/devices, boot order per media.

## Accept

- fmt, clippy, tests.
- Live A: `zz-parity-M2e-multi` from "Other install media" with two disks
  (1 GiB + 2 GiB), two VIFs on the same network, network boot; readback
  shows two VDIs with sizes, two VIFs devices 0/1, boot order `n…`.
  Destroy with disks.
- Live B (regression of M2c): `zz-parity-M2e-debian11` from Debian 11
  Legacy through the wizard with Guest Setup (free IP checked by ping
  first, user `paritytester`, temp SSH key); SSH succeeds within 5
  minutes. Destroy with disk. Delete temp secrets.
- Screenshots in `docs/supervision/evidence/M2e/`: Storage page with two
  disks, Networking page with two VIFs, Installation Media page.
