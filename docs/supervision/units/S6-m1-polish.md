# Unit S6 — M1 visual polish before release

Read `docs/supervision/CHARTER.md` first. It is binding. Fix only the items
below; each comes from the supervisor's review of S4/S5 screenshots.

1. **Context menus too tall** (carried from S4, not done in S5): items are
   ~38 px and the VM menu runs off the window. Items 22 px high (same as
   menu bar drop-downs), one font size for every item including submenu
   parents ("Start on Server ▸"), separators 7 px. A menu taller than the
   window must stay inside it (open upward or scroll). Screenshot the VM
   context menu fully visible.
2. **Notification rows too tall**: Alerts and Events rows ~38 px; make
   collapsed rows 24 px (single line: expander, icon, title, server, date,
   Actions ▾). The expanded body shows below the title.
3. **Alert severity icons**: every alert shows the warning icon. Map XenAPI
   `message.priority` to upstream severities (1 Critical/error icon,
   2 Major, 3 Warning, 4 Minor, 5 Information/info icon), as
   `XenAdmin/Alerts/Types/Alert.cs` / `MessageAlert.cs` do. VM_STARTED,
   VM_SHUTDOWN, VM_SNAPSHOTTED etc. (priority 5) show the info icon. Make
   Filter by Severity use the same five levels.
4. **Alert titles**: upstream shows friendly titles (e.g. "VM started"
   with body "VM 'x' started on server 'y'"), from `FriendlyNames.resx`
   `Message.name-*` keys. Use them where a key exists; fall back to the
   raw name.
5. **Timestamps**: alert dates appear ~7 hours in the future (08:34 when
   local time was 03:06). Parse XenAPI datetimes as UTC (`...Z` or
   `yyyyMMddTHH:mm:ssZ`) and display in local time, in Alerts, Events,
   host "Metrics updated" and anywhere else the app formats XenAPI times.
   Add a unit test for the parse/convert.
6. **Events table overflows** to the right (Actions column cut off). The
   table must fit the pane width; Message column takes the remaining width.
7. The Server / Pool column must show the pool or host name (e.g.
   "OIIO-Server"), not the connection's label.

## Accept

- fmt, clippy, tests pass.
- Screenshots in `docs/supervision/evidence/S6/`: VM context menu fully
  visible, Alerts with mixed severities and correct local times, Events
  with a completed task.
