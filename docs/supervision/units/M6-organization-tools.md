# Unit M6 — Organization views, Tools > Options, Help, small menu gaps

Read `docs/supervision/CHARTER.md` first. It is binding. Backend changes
are allowed.

## Reference (`/home/tinnerkun/Desktop/xen/upstream-xenadmin/XenAdmin/`)

- Organization: `Dialogs/NewTagDialog.cs`, `Dialogs/FolderChangeDialog.cs`,
  `Dialogs/NewCustomFieldDialog.cs`, `XenModel/XenAPI-Extensions/` and
  `Core/Folders.cs` / `Tags.cs` / `CustomFields` (stored in pool
  `gui_config` / object `other_config` keys `folder`, `XenCenter.CustomFields.*`,
  `tags` field). Search: `XenSearch/` (QueryScope, grouping, `.xensearch`
  save format), `Dialogs/NewSearch*`/`SearchFor` controls.
- Options: `Dialogs/OptionsDialog.cs` and `Dialogs/OptionsPages/`
  (Security, Display, Confirmations, Consoles, Connection, Save and
  Restore, External Tools; Plugins omitted; Updates omitted).
- Help menu and About: `Dialogs/AboutDialog.cs`.

## Deliver

1. **Tags, folders, custom fields** end to end, upstream data format so a
   pool shared with XCP-ng Center shows the same organization:
   - Edit Tags… dialog (NewTagDialog: list with checkboxes, New, Rename,
     Delete) from context menu and Properties General page.
   - Folders: Move to Folder (FolderChangeDialog), New Folder, Rename,
     Delete; "Objects by Folder" view lists the folder tree with objects;
     drag & drop of objects onto folders in that view if feasible.
   - Custom fields: New/Delete custom field definitions (text/date), values
     on Properties > Custom Fields page, "Objects by Custom Field" view.
2. **Search**: New Search… builder (object type, conditions on name, tag,
   folder, power state, memory, OS, custom field; group by; columns),
   Save Search (`.xensearch` compatible where practical) shown under
   Saved Searches, Import Search… / Export Search…, Edit/Delete saved
   search. Search tab header uses it.
3. **Tools > Options** dialog on the vertical-tab layout from M2a: pages
   Security (certificate checking — show as in upstream but cert
   management itself stays excluded), Display (show/hide timestamps,
   graph type), Confirmations (dismiss alerts/events confirmations), Consoles
   (send Windows key, clipboard, scale), Connection (proxy settings,
   timeout), Save and Restore (save server list, master password —
   master password excluded if it needs crypto we don't have; disabled
   with reason), External Tools. Persist in the app settings.
4. **Help menu**: Help Topics (open upstream online docs URL or our
   README), View Xen Desktop Log Files (open log folder), Xen Desktop on
   the Web (repo URL), Check for Updates (GitHub releases API on
   `ISWORKNEXT/xenadmin`, compare version, open release page), About
   dialog (version, commit, licence, credits to XCP-ng Center/Citrix
   XenCenter BSD-2 origin). Plugins online: omit.
5. **Small menu gaps** from `menu-status.tsv`: View > Zoom In/Out/Reset
   and Theme if still unwired, Server > Forget Password, Destroy (host
   from pool, untested-lab), Server Status Report… (host.bugreport_upload
   is out; implement local download via `/system-status` HTTP if
   available, else disabled), VM > Start in Recovery Mode, Disable
   Changed Block Tracking, Templates > Quick Create (Instant VM).
6. **Refresh `docs/supervision/menu-status.tsv`**: re-audit every row
   against the current code (several rows are stale, e.g. Install Guest
   Tools and Move VM are implemented), fix the column layout (some rows
   are shifted), and add a `verified` column (live / code / untested-lab).

## Accept

- fmt, clippy, tests (tag/folder/custom-field encode/decode against
  upstream key formats; saved search round trip).
- Screenshots in `docs/supervision/evidence/M6/`: Edit Tags dialog,
  Objects by Folder view, New Search builder, Options dialog, About.
- Live on .170: tag a `zz-parity-M6-vm` with `zz-parity-tag`, put it in
  folder `/zz-parity`, set a custom field `zz-parity-field`; readback of
  `tags`, `other_config.folder`, custom field key; views show it. Remove
  all of these afterwards (VM, tag, folder, custom field definition).
## Supervisor additions (from M5 review)

- `cargo build -p xen-core` alone fails: chrono is used with the `clock`
  feature but xen-core's Cargo.toml does not enable it (workspace builds
  pass by feature unification). Enable the feature and confirm
  `cargo build -p xen-core` and `cargo test -p xen-core` pass alone.
- `.xensearch` import/export currently handles only our native queries.
  Since item 2 builds the search model, translate upstream `.xensearch`
  query XML for the conditions item 2 supports; report the rest.
