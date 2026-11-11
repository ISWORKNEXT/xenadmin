# Unit S5 — Notifications: Alerts and Events

Read `docs/supervision/CHARTER.md` first. It is binding.

## Why

The Notifications navigation button still shows our old "Activity & tasks"
page. XCP-ng Center's Notifications mode shows a small tree on the left
(Alerts, Events — with counts) and a full-width page on the right. This
unit ports it. It is the last unit of milestone M1.

## Reference

`/home/tinnerkun/Desktop/xen/upstream-xenadmin/XenAdmin/TabPages/`:
`NotificationsBasePage.cs`, `AlertSummaryPage.cs` (+ `.Designer.cs`,
`.resx`), `HistoryPage.cs` (+ designer, resx). Also
`XenAdmin/Controls/NotificationsView.cs` (left list) and
`XenAdmin/Alerts/` for alert sources and severity.

## Deliver

1. **Notifications mode left pane**: replaces the tree with a list of
   "Alerts (N)" and "Events (N)" (upstream `NotificationsView`). Updates is
   left out (excluded: no update/licensing server in this port) — do not
   show it. Counts are unread/undismissed items.
2. **Alerts page** (`AlertSummaryPage`): toolbar Filter by Severity ▾,
   Filter by Server ▾, Filter by Date ▾, Refresh, Export, Dismiss ▾
   (Dismiss All, Dismiss Selected). Table columns: expander, severity icon,
   Message (title, expands to full body), Server / Pool, Date, Actions ▾
   (Help / Dismiss / Go To). "Showing first 1000 entries" label when capped.
   Sources: XenAPI `message` objects (alarms, perf alerts, HA, etc.) the
   app already loads or can load with `message.get_all_records`; this is
   allowed in this unit. Dismiss calls `message.destroy` (reuse the existing
   `DismissAlert` handler). Export writes CSV like upstream.
3. **Events page** (`HistoryPage`): toolbar Filter by Status ▾ (In
   progress, Completed, Failed, Cancelled), Filter by Server ▾, Filter by
   Date ▾, Dismiss ▾. Columns: expander, status icon, Message (title +
   details on expand, progress bar while running), Server / Pool, Date,
   Actions ▾ (Cancel for running tasks that allow it, Dismiss, Go To).
   Source: the app's existing task/activity history plus XenAPI tasks.
4. **Status bar** (S1): the right side shows the running task with progress
   like upstream; clicking it opens Events.
5. Remove the old Activity & tasks page once both pages cover its content.
   Keep any command it offered that has no other place (report it).

## Out of scope

Updates/patching, alert threshold configuration (M7), email alerts,
the Performance alerts dialog.

## Accept

- fmt, clippy, tests pass.
- Screenshots in `docs/supervision/evidence/S5/`: Notifications mode with
  Alerts page, Events page with a completed and a failed task.
- Real clicks on the dev host: produce a task (e.g. snapshot a
  `zz-parity-S5-*` VM) and see it appear running then completed in Events;
  produce a failed task (e.g. start a VM with no disks/boot device if it
  fails) and see it under Failed filter. If the host has any alert message,
  dismiss one you created with `message.create` (name `zz-parity-S5`),
  and confirm via XenAPI that it is gone. Clean up.

## Supervisor additions (from S4 review) — do these first

1. Context menus are far too tall: each item is ~38 px and a VM menu runs
   off the bottom of the window. Make menu items 22 px high (same as the
   menu bar drop-downs), one font size for every item (submenu parents
   "Start on Server ▸" / "Resume on Server ▸" are currently larger), and
   separators 7 px. A menu taller than the window must open upward / scroll
   the way egui menus do, never be cut off.
2. Labels are generated from command ids in some places ("Migrate V M").
   Every context menu item must use the upstream display text from
   `menubar.rs` / upstream resx (e.g. "Migrate to Server ▸"). Add a unit
   test that no context-menu label contains a single-letter word split
   from an id.
3. Halted VM selected: the toolbar shows Shut Down / Reboot / Suspend
   instead of Start / Reboot / Suspend (S3a screenshots showed Start). Fix
   the toolbar to follow upstream visibility (Start when halted, Shut Down
   when running), even when Start is disabled for that VM.
