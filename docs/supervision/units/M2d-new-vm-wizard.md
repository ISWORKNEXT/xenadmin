# Unit M2d — New VM wizard in upstream layout

Read `docs/supervision/CHARTER.md` first. It is binding.

## Why

Our New VM wizard is a modal with a breadcrumb (Source › Name › Resources ›
Storage & media › Networking › Guest Setup › Review). Upstream's is a
wizard window with a step list on the left and pages that match
`XenAdmin/Wizards/NewVMWizard/`. This unit restyles and reorders it; the
backend (`provisioning.rs`, guest setup from M2c) stays.

## Reference (`/home/tinnerkun/Desktop/xen/upstream-xenadmin/XenAdmin/Wizards/`)

`NewVMWizard/NewVMWizard.cs` (page order and conditions), `Page_Template`,
`Page_CopyBiosStrings`, `Page_Name`, `Page_InstallationMedia`,
`Page_HomeServer`, `Page_CpuMem`, `Page_Storage`, `Page_Networking`,
`Page_CloudConfigParameters`, `Page_Finish` (+ resx for texts). Also the
shared wizard frame `XenWizardBase.cs` / `Controls/Wizard*` for the look:
header strip with page title + description, step list on the left with the
current step bold, Previous / Next / Cancel bottom-right.

## Deliver

1. **Shared wizard frame** (one module, reused later by New SR, New
   Network, Import/Export, New Pool): window ~860×620, left step list
   (200 px), header with page title and one-line description, body,
   footer Previous / Next (Finish on last) / Cancel. Steps not yet
   reachable greyed. Esc = Cancel with confirm when data was entered.
2. **New VM pages in upstream order**: Template (searchable list grouped
   by OS like upstream, with description panel), [Copy BIOS strings when
   upstream shows it], Name (name, description), Installation Media
   (ISO library / physical DVD / network boot / boot params, hidden for
   installed-OS templates as upstream does via template provisioning),
   Home Server (place VM on this server / don't assign, with host list and
   free memory), CPU & Memory (vCPUs, topology, memory), Storage (disk
   list with Add/Edit/Delete and "diskless" option, SR choice), Networking
   (VIF list Add/Properties/Delete), then our **Guest Setup** page (M2c)
   in the position of upstream's Cloud Config page, Finish (summary list
   + "Start the new VM automatically").
3. Keep all existing provisioning behaviour and the M2c Guest Setup
   validation. Keep the Ctrl/keyboard flow working (Enter = Next).
4. Also move the **New SR wizard** (if it exists in the app) onto the same
   frame without changing its pages (M3 will redo pages).

## Accept

- fmt, clippy, tests.
- Screenshots in `docs/supervision/evidence/M2d/`: Template page, Home
  Server page, Storage page, Finish page.
- Live: create `zz-parity-M2d-vm` from a stock XCP-ng template (e.g.
  "Other install media") with a 1 GiB disk and one VIF, not started;
  XenAPI readback of name, vCPUs, memory, disk size, VIF network.
  Destroy it with its disk.
