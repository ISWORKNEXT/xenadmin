# Unit M2a — Properties dialog with vertical tabs

Read `docs/supervision/CHARTER.md` first. It is binding.

## Why

Upstream has one Properties dialog per object: a vertical tab list on the
left (icon, page title, one-line summary of the current value) and the
edit page on the right, OK / Cancel at the bottom, all changes applied
together on OK. The app has a small name/description/tags modal
(`native/src/views/properties.rs`) plus separate editors (VM Resources,
HA edit, Pool options, disk resize/mode/bootable). This unit folds them
into the upstream dialog. It is the first unit of milestone M2.

## Reference

- `/home/tinnerkun/Desktop/xen/upstream-xenadmin/XenAdmin/Dialogs/PropertiesDialog.cs`
  lines 120–300: which pages each object type gets, in order.
- Pages under `XenAdmin/SettingsPanels/` (`GeneralEditPage`,
  `CustomFieldsDisplayPage`, `CpuMemoryEditPage`, `BootOptionsEditPage`,
  `VMHAEditPage`, `PerfmonAlertEditPage`, `HomeServerEditPage`,
  `VMAdvancedEditPage`, `HostAutostartEditPage`, `LogDestinationEditPage`,
  `HostPowerONEditPage`, `PoolAdvancedEditPage`, `NetworkOptionsEditPage`,
  `EditNetworkPage`, `VDISizeLocationPage`, `VBDEditPage`,
  `LivePatchingEditPage`, `SrReadCachingEditPage`, `HostMultipathPage`)
  and their `.resx` for labels and summary text (`SubText`).

## Deliver

1. **Dialog frame**: resizable window ~720×540, vertical tab list (240 px)
   with icon + title + grey `SubText` line, page on the right, OK / Cancel
   bottom-right. OK applies every changed page in order and closes; on a
   failure, keep the dialog open, select the failing page and show the
   error there. Cancel discards. Title "'<name>' Properties".
2. **Pages per type** (upstream order; a page whose backend does not exist
   is shown with its fields disabled and "Not implemented yet", except
   RBAC/AD/certificate pages which are omitted):
   - VM: General (name, description, folder, tags), Custom Fields, CPU
     (vCPUs, topology, priority), Memory if upstream shows it here, Boot
     Options (order, OS boot parameters, firmware/secure boot as upstream),
     High Availability, Alerts, Home Server, GPU, USB, Advanced
     (optimization/shadow multiplier), plus auto-power-on where upstream
     puts it. Reuse `update_vm_resources`, `update_vm_ha` and existing
     tag/folder setters.
   - Host: General, Custom Fields, Alerts, Multipathing, Log Destination,
     Power On, plus the pool options page for a standalone host.
   - Pool: General, Custom Fields, Alerts, GPU, Security, Live Patching,
     Network Options, Clustering, Advanced — map the existing Pool options
     dialog (compression, IGMP snooping, live patching, VM autostart) onto
     these pages; remove the separate Pool options dialog and the
     "Pool options… (VM autostart)" button.
   - SR: General, Custom Fields, Alerts, Read Caching.
   - VDI (from the VM Storage tab Properties button): General, Custom
     Fields, Size and Location (resize), one "<VM name>" device page per
     VBD (mode, bootable, position disabled if no setter).
   - Network: General, Custom Fields, Network Settings (MTU, auto-add) as
     far as the backend supports.
3. **Entry points**: menu bar Properties, context menu Properties, the
   General tab's Properties button, the Storage/Networking table
   Properties buttons, and General tab section "Edit…" buttons (which open
   the dialog on the matching page). Remove the old VM Resources dialog
   and separate HA edit dialog once their fields live here.
4. Keep the `vm-data` work out of this unit (M2c).

## Out of scope

Copy/Move/Delete wizards (M2b), new backend setters beyond trivial
`VM.set_*`/`other_config` writes that a page needs (allowed if one call,
with a readback), RBAC/certificates.

## Accept

- fmt, clippy, tests pass.
- Screenshots in `docs/supervision/evidence/M2a/`: VM Properties on
  General, on CPU, on Boot Options; Pool Properties on Advanced/Live
  Patching; VDI Properties on the device page.
- Real clicks on a `zz-parity-M2a-*` VM: change name + vCPUs + boot order
  in one OK, XenAPI readback shows all three; change pool VM autostart
  on, read back, set it back off. Clean up.
- Update `docs/supervision/menu-status.tsv` for changed items.
