# Unit S3b — Host, pool, SR, VDI, network pages and the Search tab

Read `docs/supervision/CHARTER.md` first. It is binding.

## Why

S3a built the shared page chrome (title strip, `tabs_for(selection)`) and the
VM / template / snapshot tabs. Hosts, pools and SRs still show the old
pages, and group rows ("VMs", a tag, "Objects") show nothing. This unit
finishes the tab sets so every tree node has an upstream-shaped page.

## Reference

- `docs/supervision/upstream-tabs-tree.md` section 1 (tabs per selection).
- Upstream pages in `/home/tinnerkun/Desktop/xen/upstream-xenadmin/XenAdmin/TabPages/`:
  `GeneralTabPage.cs`, `BallooningPage.cs`, `PhysicalStoragePage.cs`,
  `SrStoragePage.cs`, `NetworkPage.cs`, `NICPage.cs`, `GpuPage.cs`,
  `UsbPage.cs`, `SearchPage.cs`, `HomePage.cs`. Copy section names, column
  names, button labels, button order and enable rules.

## Deliver

Reuse the S3a chrome and tab bodies. Do not fork them.

1. **Host** (live): General, Memory, Storage, Networking, NICs, GPU, USB
   (only when the host has PUSBs), Console, Performance, Search.
   Disconnected host: General, Search.
   - General sections in upstream order: General, Management Interfaces,
     Memory, Version Details, CPUs, Updates, License Details (omit rows the
     app has no data for; no licence actions).
   - Memory: host memory bar (total / used / control domain) and the list of
     VMs with their memory bars.
   - Storage (physical): SR table (Name, Description, Type, Shared, Usage,
     Size, Virtual allocation) with New SR…, Trim… / Reclaim, Properties.
   - Networking: network table (Name, Description, NIC, VLAN, Auto, Link
     Status, MAC, MTU) with Add Network…, Properties, Remove; below it the
     IP Address Configuration table with Configure….
   - NICs: PIF table (NIC, MAC, Link Status, Speed, Duplex, Vendor, Device,
     PCI Bus Path, FCoE, SR-IOV) with Create Bond…, Delete Bond, Rescan.
   - GPU / USB: tables from existing inventory data; buttons without a
     backend are disabled with "Not implemented yet".
   - Console and Performance: the existing host views filling the tab.
2. **Pool**: General, Memory, Storage, Networking, GPU, Search — the host
   bodies fed with pool-wide data.
3. **SR**: General (General, Status, Multipathing sections), Storage
   (virtual disk table: Name, Description, Size, VM; buttons Add…, Rescan,
   Move…, Delete, Properties), Search.
4. **VDI**: General, Search. **Network**: Search only.
5. **Search tab** on every selection, and the only tab for group rows
   (Objects groups, a tag, folders, saved searches, multi-level roots):
   - Results table with upstream default columns: Name (with icon and
     description line), CPU Usage, Used Memory, Disks, Network, Address,
     Uptime. Expandable rows grouped as upstream does for the default
     overview (pool → host → VM).
   - Scope = the selected node and its descendants; a group row lists its
     members. Double-click a row selects that object in the tree.
   - The existing search/filter UI moves into this tab's header (New
     Search…, Saved Searches ▾, Export…, Import…; unbacked ones disabled).
6. **Home page** (root selected): Home, Search tabs. Keep the existing Home
   content but drop anything duplicated by the menu bar or toolbar.
7. **Remove** the old host / pool / SR page headers, button rows and cards
   once their commands are reachable from the menu bar, toolbar or a tab
   button; add a tab button where upstream has one.
8. Any item listed under "FOUND, NOT FIXED" in the S3a report that the
   supervisor appended below.

## Out of scope

Context menus (S4), Notifications (S5), new backend functions, new dialogs,
HA/WLB/AD tabs, Docker tabs, plugins.

## Accept

- fmt, clippy, tests pass.
- Real app on the dev host; screenshots in `docs/supervision/evidence/S3b/`:
  host General, host Networking, host NICs, SR Storage, Search tab on the
  root, Search tab on the Objects > VMs group.
- With real clicks: SR Storage tab > Add… creates a 1 GiB
  `zz-parity-S3b-*` disk (XenAPI readback) and Delete removes it;
  double-clicking a VM row in the Search tab selects that VM.
- Update `docs/supervision/menu-status.tsv` if any item changes state.

## Supervisor additions from S3a review

(appended after S3a is accepted)

From the S3a screenshots (fix these on the shared chrome and VM tabs too):

1. Tab strip: the labels run together like plain text. Render real tabs:
   12 px horizontal padding, 28 px high, hover background, selected tab
   filled or underlined with a bottom border across the whole strip.
2. VM Storage tab: replace the old "CD/DVD drives" card with the upstream
   DVD drive row at the top of the tab (`VMStoragePage`: "DVD Drive N:"
   ISO combo with Eject / New CD-DVD Drive). Remove the "More ▾" button
   (upstream has none). Fill Position (`userdevice`), Device Path
   (`device`) and Priority (`qos_algorithm_params`) when the data is
   available.
3. VM General: the High Availability "Restart priority" row shows a stray
   empty widget beside Edit…; show the value as text like other rows.
4. Snapshots tab: drop the card frame; buttons and list fill the page like
   the Storage and Networking tabs.
5. One text size for label/value rows and table cells (currently label rows
   are larger than table text).
