# Unit S2 — Navigation pane, Infrastructure tree, selection model

Read `docs/supervision/CHARTER.md` first. It is binding.

## Why

S1 gave the window the upstream menu bar and toolbar. The left pane is still
our own design: category folders ("Hosts", "Virtual machines", "Templates",
"Storage", "Networks"), "Infrastructure" and "Activity & tasks" pseudo-rows,
and closable object tabs ("Home | vm-name ×") on the right. XCP-ng Center
has one tree whose selection drives everything. This unit ports that.

## Reference

- `docs/supervision/upstream-tabs-tree.md` sections 2 and 3 (tree building,
  sort order, View toggles, navigation modes, labels, counts), with upstream
  file/line citations. Upstream source is at
  `/home/tinnerkun/Desktop/xen/upstream-xenadmin`.

## Deliver

1. **Infrastructure tree** (default mode), replacing the category folders:
   - Root node "Xen Desktop" (home icon). Selecting it shows the Home page.
   - Under root: one node per connection — the pool node when the pool is
     named or has more than one host, otherwise the standalone host node
     directly (upstream `Helpers.GetPool` semantics). A disconnected
     connection shows as a greyed host node.
   - Under a pool: its hosts, then pool-level objects (shared SRs, templates,
     halted VMs with no home host). Under a host: VMs whose home is that host
     (`resident_on` when running/paused, else storage/affinity host) and SRs
     local to that host.
   - Sort: pool, host, VM, then other types; natural name order within a type.
   - Not shown in this mode: snapshots, VDIs, networks, control domains, the
     tools SR.
   - Each node: type/state icon (running, halted, suspended, paused VM;
     host live / maintenance / disconnected; SR default / detached / broken;
     template) and name. No counts, no badges.
   - Expand/collapse state persists while the app runs. Newly connected
     pools/hosts start expanded.
2. **View menu toggles wired** (replace the S1 disabled items): Custom
   Templates (default on), Xen Desktop Templates (default off), Local Storage
   (default on), Hidden Objects (default off; `other_config.HideFromXenCenter`).
   Persist them in the existing app settings.
3. **Search box** above the tree filters nodes by name (keep parents of
   matches). Keep Ctrl+K focusing it.
4. **Navigation buttons** stacked at the bottom of the left pane, in this
   order and with these labels: Infrastructure, Objects, Organization Views
   (drop-down: Objects by Tag, Objects by Folder, Objects by Custom Field,
   vApps), Saved Searches (drop-down listing saved searches), Notifications
   (with total count when > 0). The active mode is highlighted.
   - Objects: root "Objects", grouped by type (Pools, Servers, VMs,
     Templates, Snapshots, Storage, Virtual Disks, Networks).
   - Objects by Tag: root "Tags", one group per tag, objects under it.
   - Objects by Folder / by Custom Field / vApps: build from real data
     (`other_config` folder key, custom-field keys, `VM_appliance`) if the
     inventory already carries it; otherwise show the root with an empty-state
     line. Do not add inventory fetching for these in this unit.
   - Saved Searches: selecting one shows its results using the existing
     search results view.
   - Notifications: shows the existing Activity & tasks page for now (S5
     replaces it). Count = failed tasks.
5. **Selection model.** Remove the closable object-tab strip ("Home | name ×").
   The right pane always shows the page of the single selected tree node,
   using the existing detail pages unchanged (S3 will restructure them).
   Back/Forward (S1) walk this selection history. The menu bar and toolbar
   read the same selection. Selecting a node in Objects / Tag views selects
   the same underlying object. Ctrl+W and tab-related shortcuts are removed;
   update the shortcut hint line accordingly or remove it.
   Startup flags that opened an object (`--host`, `--vm`, …) now select it.
6. **S1 polish** (from supervisor review of the S1 screenshots):
   - Menu bar and drop-down item text use the same size as toolbar text
     (currently noticeably larger); add 8 px left padding to the menu bar.
   - Rename "Install XenServer VM Tools…" to "Install Guest Tools…".
   - Toolbar order when a halted VM is selected should still show the
     upstream trio positions: [Start] [Reboot] [Suspend] then Pause — keep,
     but hide Pause/Unpause unless the VM is running/paused, as upstream does.

## Out of scope

Tab sets and page content (S3), right-click context menus (S4), multi-select,
drag and drop, Notifications content (S5), any backend or inventory change
beyond reading fields already fetched.

## Accept

- fmt, clippy, tests pass.
- Real app on the dev host; screenshots in `docs/supervision/evidence/S2/`:
  (a) Infrastructure tree expanded with a VM selected, (b) Objects mode,
  (c) Objects by Tag, (d) View > Xen Desktop Templates toggled on showing
  templates in the tree.
- With real clicks: select a VM in the tree → its page shows and VM menu
  items enable; select the host → Server menu items enable; Back returns to
  the VM. No live mutation is needed in this unit.
- Update `docs/supervision/menu-status.tsv` rows for the four View items.
