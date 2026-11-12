# Unit M5 — Import / Export wizards

Read `docs/supervision/CHARTER.md` first. It is binding. Backend changes
are allowed.

## Reference (`/home/tinnerkun/Desktop/xen/upstream-xenadmin/XenAdmin/Wizards/`)

- `ImportWizard/`: ImportSourcePage, ImportEulaPage, ImportSelectHostPage
  (GlobalSelectHost), ImportSelectStoragePage / StoragePickerPage,
  ImportSelectNetworkPage / NetworkPickerPage, ImportSecurityPage,
  ImportOptionsPage, ImageVMConfigPage, ImportBootOptionPage,
  ImportFinishPage; `ImportWizard.cs` decides page flow per type
  (XVA, OVF/OVA, disk image VHD/VMDK/raw).
- `ExportWizard/`: ExportSelectVMsPage, ExportAppliancePage (format,
  location), ExportEulaPage, ExportOptionsPage (manifest, signing,
  compression, encryption — signing/encryption need certificates:
  excluded, show disabled), ExportFinishPage.
- `XenOvfApi` / `XenOvfTransport` in the upstream repo for OVF layout.

Existing backend: `xva.rs` (`transfer_xva`, XVA import/export over HTTP).

## Deliver

1. **Import wizard** on the shared wizard frame (File > Import…, and
   dropping a file on the window if easy). Source page detects type by
   extension. Flows:
   - XVA: Source → Home server → Storage → Networking → Finish (start VM
     option). Reuse `transfer_xva`.
   - Disk image (VHD, VMDK, raw .img/.raw): Source → VM config (name,
     vCPUs, memory) → Home server → Storage → Networking → Boot options →
     Finish. Create VM, create VDI, upload via `/import_raw_vdi`
     (format=vhd or raw) with progress and cancel.
   - OVF/OVA: Source (manifest/EULA pages when present) → Home server →
     Storage per disk → Networking per NIC → Options (run OS fixup:
     disabled "Not implemented yet" if no fixup ISO) → Finish. Parse
     OVF XML, unpack OVA (tar), upload each disk with `/import_raw_vdi`,
     create VM and VIFs from the envelope.
2. **Export wizard** (VM > Export…, File > Export…): Select VMs → format
   XVA (single VM) or OVF/OVA (one or many) → location/file name →
   Options (manifest; signing/encryption disabled "Excluded from this
   port") → Finish. XVA via existing path; OVF writes descriptor + disks
   via `/export_raw_vdi` (vhd), OVA packs them into a tar.
3. **File menu**: Export Server List… / Import Server List… (upstream
   XML connection list format, no passwords), Import/Export Search
   (saved search `.xensearch` files) if search persistence exists;
   otherwise leave disabled for M6.
4. Progress shows in Events (S5) with cancel; partial objects are removed
   on cancel/failure (VM, VDIs created by this import).

## Accept

- fmt, clippy, tests (OVF parse/write round trip unit test, VHD footer
  detection test).
- Screenshots in `docs/supervision/evidence/M5/`: import source page,
  import storage page, export format page.
- Live on .170: export a `zz-parity-M5-src` VM (1 GiB disk, diskless OS is
  fine) as XVA and as OVA to a temp dir; import both back as
  `zz-parity-M5-xva` and `zz-parity-M5-ova`; import a 64 MiB raw image you
  create with `truncate` as `zz-parity-M5-img`. XenAPI readback of each
  VM's disk sizes and VIF count. Cancel one import midway and confirm no
  partial VM/VDI remains. Clean up every object and temp file.
