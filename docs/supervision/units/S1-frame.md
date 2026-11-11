# Unit S1 — Main window frame, menu bar, toolbar, status bar

Read `docs/supervision/CHARTER.md` first. It is binding.

## Why

The app currently has a custom undecorated title bar, a breadcrumb/context
toolbar (`native/src/views/chrome.rs`), and scattered "Manage…" / "More" /
"Host actions…" menu buttons. Users of XCP-ng Center cannot find commands.
This unit gives the window the upstream frame. Later units (S2–S5) redo the
tree, tabs, context menus and notifications; do not start them.

## Reference

- `docs/supervision/upstream-shell.txt` — the exact upstream menu bar and
  toolbar tree with the command class behind each item.
- Upstream source: `/home/tinnerkun/Desktop/xen/upstream-xenadmin/XenAdmin/MainWindow.cs`,
  `MainWindow.Designer.cs`, and `Commands/*.cs` (`CanRunCore` gives the
  enable rule for each command).

## Deliver

1. **OS window decorations.** Remove the custom title bar, drag region and
   resize handles; use the native title bar titled "Xen Desktop".
2. **Menu bar** as the top row: File, View, Pool, Server, VM, Storage,
   Templates, Tools, Window, Help — every item, submenu and separator from
   `upstream-shell.txt`, same labels and order. Replace `{0}` with
   "Xen Desktop". Skip `PluginItemsPlaceHolder`, Docker items, and
   Conversion Manager.
   - An item whose function already exists in the app (there are ~180
     `UiAction` variants; search them) dispatches that existing action
     against the current selection.
   - An item with no implementation yet is shown disabled with hover text
     "Not implemented yet". Certificate and "Reconnect As…" items are shown
     disabled with hover text "Out of scope".
   - Enabled state follows the current selection, following the upstream
     command's `CanRunCore` where the app has the data (e.g. VM menu items
     need a selected VM in a suitable power state; Storage > Set as Default
     needs a selected SR). When disabled because of selection/state, the
     hover text says why in one short sentence.
   - Put the menu model in one new module (e.g. `native/src/views/menubar.rs`)
     as data: label, command id, enabled/reason function, action builder.
     S4 will reuse this for context menus, so keep it selection-driven and
     free of layout code.
3. **Toolbar** under the menu bar, matching upstream order: Back, Forward |
   Add New Server | New Pool, New Storage, New VM | Shut Down, Reboot,
   Suspend (and Start / Resume / Power On / Pause / Unpause / Force Shut
   Down / Force Reboot shown per selection state exactly as upstream swaps
   them). Icon above or beside a text label, using existing icon helpers.
   Back/Forward walk selection history. View > Toolbar toggles it.
4. **Status bar** at the bottom: left = status text of the latest task /
   refresh, right = progress indicator while a task runs.
5. Remove the old breadcrumb/context toolbar row and the per-page "Add
   connection", "Refresh", "More", "Host actions…", "Manage storage…",
   "Manage network…" entry points *only where the same command is now
   reachable from the menu bar*. Keep anything not yet reachable.
6. Theme toggle and zoom move to View menu (below Toolbar, after a separator).
   Keyboard shortcuts stay working.

## Out of scope

Tree structure, tab sets, context menus, notifications, any new backend
function, any new dialog. No behavior change to existing dialogs.

## Accept

- fmt, clippy, tests pass.
- Launch the real app connected to the dev host. Screenshots in
  `docs/supervision/evidence/S1/`: (a) main window with a VM selected,
  (b) VM menu open showing enabled/disabled items, (c) Server menu open with
  a host selected.
- From the menu bar, with real clicks: VM > Start/Shut down on a temporary
  `zz-parity-S1-*` VM changes its power state (read back via XenAPI), and
  Storage > Set as Default is disabled with a reason when no SR is selected.
- Append to the report a table: every menu item → `wired` /
  `disabled-not-implemented` / `disabled-out-of-scope`. Write the same table
  to `docs/supervision/menu-status.tsv` (columns: menu, item, upstream_command,
  status, ui_action).
