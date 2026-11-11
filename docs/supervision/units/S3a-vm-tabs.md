# Unit S3a — Tab framework and VM / template / snapshot tabs

Read `docs/supervision/CHARTER.md` first. It is binding.

## Why

S1 and S2 gave the app the upstream frame and tree. The right pane still
shows our old pages: a big header with Network/Disks/Console buttons, rows of
action buttons (Export XVA…, Full copy…, Start, Start on…, Shut down, More…,
Take snapshot, Resources…), a "Quick actions" card, and two-column cards.
Those commands now live in the menu bar and toolbar. This unit replaces the
VM page with the upstream tab set and page layouts. S3b does hosts, pools,
SRs and the Search tab; do not start it.

## Reference

- `docs/supervision/upstream-tabs-tree.md` section 1: which tabs each
  selection gets and in what order.
- Upstream pages in `/home/tinnerkun/Desktop/xen/upstream-xenadmin/XenAdmin/TabPages/`:
  `GeneralTabPage.cs`, `BallooningPage.cs`, `VMStoragePage.cs`,
  `NetworkPage.cs`, `SnapshotsPage.cs`, `PerformancePage.cs`. Copy their
  section names, column names, button labels, button order and enable rules.

## Deliver

1. **Page chrome shared by all object pages** (one module, reused by S3b):
   - Title strip: type/state icon, object name, and on the right
     "Logged in as: <user>" for the connection. One line, compact. Remove the
     big header, UUID line and header buttons.
   - Tab strip directly below, built from a single function
     `tabs_for(selection) -> Vec<Tab>` implementing the reference table.
     Remember the last selected tab per object type; fall back to General.
2. **VM tabs** (real VM): General, Memory, Storage, Networking, Console,
   Performance, Snapshots. Search is added by S3b.
   - **General**: "Properties" button top-left, "Expand all / Collapse all"
     top-right, then single-column collapsible sections with label/value
     rows, in upstream order: General (name, description, tags, folder,
     operating system, virtualization mode, BIOS strings copied, virtualization
     state, time since startup, UUID), Boot Options (auto-boot, boot order /
     OS boot parameters), CPUs (vCPUs, topology, priority), Read Caching,
     High Availability (restart priority), Memory & Home Server where upstream
     shows them. Use the data the app already has; omit a row whose data the
     app does not have. Right-click a value → Copy.
     Properties opens the existing Properties/Resources dialog.
   - **Memory**: the VM's memory bar (used / dynamic min–max / static max)
     and an "Edit…" button opening the existing memory editor.
   - **Storage**: virtual disk table (Position, Name, Description, SR,
     SR Volume / Size, Read Only, Priority, Active, Device Path) and below it
     buttons in upstream order: Add…, Attach Disk…, Activate/Deactivate,
     Move…, Detach, Delete, Properties. Wire each to the existing action;
     enable per row selection and upstream rules.
   - **Networking**: virtual interface table (Device, MAC, Limit, Network,
     IP Address, Active) with Add Interface…, Properties, Remove,
     Activate/Deactivate.
   - **Console**: the existing console view, unchanged, filling the tab.
   - **Performance**: the existing charts, with the range selector; an
     "Actions" drop-down holding New Graph…, Edit Graph…, Delete Graph,
     Restore Default Graphs (disabled, "Not implemented yet").
   - **Snapshots**: the existing snapshot list with buttons Take Snapshot…,
     Revert To…, Actions ▾ (New VM from Snapshot…, New Template from
     Snapshot…, Export Snapshot as Template…, Properties), Delete. Enable per
     upstream `RefreshToolStripButtons`.
3. **Template** tabs: General, Memory, Storage (custom templates only),
   Networking. **Snapshot** selection: General, Memory, Networking. Reuse the
   VM tab bodies with template-appropriate buttons.
4. **Remove** the old VM page pieces listed under "Why". Before deleting any
   entry point, confirm the same command is reachable from the menu bar,
   toolbar or a tab button; if it is not, add it to the tab where upstream
   has it. A button with no backend is shown disabled with "Not implemented yet".
5. **Density and polish** (supervisor review of S2 screenshots):
   - Tree rows 24 px high; table rows 24 px; label/value rows 22 px. The
     current ~38 px rows waste half the window.
   - Objects-mode group rows use the icon of their type (pool, server, VM,
     template, snapshot, storage, disk, network), not the home icon.
   - The five navigation buttons share one style: full pane width,
     left-aligned icon + label, same height, active one filled.

## Out of scope

Host, pool, SR pages and the Search tab (S3b); context menus (S4); new
backend functions; new dialogs; Notifications.

## Accept

- fmt, clippy, tests pass.
- Real app on the dev host; screenshots in `docs/supervision/evidence/S3a/`:
  VM General, VM Storage, VM Networking, VM Snapshots, a custom template's
  General tab.
- With real clicks on a temporary `zz-parity-S3a-*` VM: Storage tab > Add…
  creates a 1 GiB disk (XenAPI readback), Snapshots tab > Take Snapshot…
  creates a snapshot and Delete removes it. Clean up the VM afterwards.
