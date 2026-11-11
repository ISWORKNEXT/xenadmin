# 1. Tab pages per selection — `MainWindow.cs` `GetNewTabPages()` (lines 1293–1392)

**Tab page labels** (`XenAdmin/MainWindow.resx`):

| Field | Label | resx line |
|---|---|---|
| `TabPageHome` | Home | 195–196 |
| `TabPageGeneral` | General | 225–226 |
| `TabPageBallooning` | Memory | 252–253 |
| `TabPageConsole` | Console | 282–283 |
| `TabPageCvmConsole` | Driver Domain Console | 306–307 |
| `TabPageStorage` | Storage | 336–337 |
| `TabPagePhysicalStorage` | Storage | 366–367 |
| `TabPageSR` | Storage | 396–397 |
| `TabPageNetwork` | Networking | 426–427 |
| `TabPageNICs` | NICs | 456–457 |
| `TabPagePeformance` | Performance | 489–490 |
| `TabPageSnapshots` | Snapshots | 543–544 |
| `TabPageGPU` | GPU | 615–616 |
| `TabPageSearch` | Search | 645–646 |
| `TabPageDockerProcess` | Processes | 672–673 |
| `TabPageDockerDetails` | Details | 705–706 |
| `TabPageUSB` | USB | 732–733 |

**Boolean flags** (`MainWindow.cs` 1295–1316; helpers in `SelectedItemCollection.cs`):

- `show_home` = selection Count == 1 **and** `Selection[0].Value == null` (i.e. the root “Overview”/XenCenter node is selected, or the tree is empty) — `MainWindow.cs:1300`. (`SelectedItem.Value` returns the `IXenObject` or `GroupingTag`, `SelectedItem.cs:198–208`.)
- `is_connected` = first item’s connection exists and `IsConnected` — `MainWindow.cs:1301`.
- `multi` = Count > 1 — `MainWindow.cs:1303`.
- `isPoolSelected` = `FirstIs<Pool>()` — `MainWindow.cs:1305`; `FirstIs<T>` = `First is T` (`SelectedItemCollection.cs:264–267`).
- `isVMSelected` = `FirstIs<VM>()` (true for real VMs, templates **and** snapshots) — `MainWindow.cs:1306`.
- `isHostSelected` = `FirstIs<Host>()` — `MainWindow.cs:1307`.
- `isSRSelected` = `First as SR != null` — `MainWindow.cs:1308–1309`.
- `isVdiSelected` = `FirstIs<VDI>()` — `MainWindow.cs:1310`.
- `isRealVMSelected` = `FirstIs<VM>() && !is_a_template` — `MainWindow.cs:1311`; `SelectedItemCollection.cs:283–289`.
- `isTemplateSelected` = `FirstIs<VM>() && is_a_template && !is_a_snapshot` (a “real” template, **excluding snapshots**) — `MainWindow.cs:1312`; `SelectedItemCollection.cs:294–300`.
- `isHostLive` = `FirstIs<Host>() && Host.IsLive()` — `MainWindow.cs:1313`; `IsLive()` = has live `Host_metrics` (`Host.cs:812–819`).
- `selectedTemplateHasProvisionXML` = `isTemplateSelected && HasProvisionXML()` — `MainWindow.cs:1316`; `HasProvisionXML()` = `other_config` contains key `"disks"` (`VM.cs:1451–1454`). This distinguishes a **default template** (has provisioning XML) from a **custom template** (doesn’t).
- `isDockerContainerSelected` = `First is DockerContainer` — `MainWindow.cs:1314`.

**Guard rules** (apply to every block): every tab except **Search** is gated by `!multi && !SearchMode` (`MainWindow.cs:1320,1323,1327,1330,1333,1336,1339,1342,1345,1356,1360,1372,1375,1378,1381`). **Search is always appended** (`MainWindow.cs:1387`). Plugin feature tabs require exactly one selected item (`MainWindow.cs:1366`).

**Display order** is the order of `newTabs.Add(...)` calls (`MainWindow.cs:1321–1387`):
Home → General → Memory → Storage(VM) → Storage(SR) → Storage(Physical) → Networking → NICs → Processes → Details → GPU → USB → [plugin console features] → Console → Driver Domain Console → Performance → Snapshots → [plugin other features] → Search.

| Selection type | Tabs shown (in order) | Exact conditions |
|---|---|---|
| **Nothing / root (“Overview”) selected** | Home, Search | `show_home` (`1300`) adds Home (`1320–1321`); all object flags false; Search always (`1387`). |
| **Pool** | General, Memory, Storage(Physical), Networking, GPU, Search | General `isPoolSelected` (`1323–1325`); Memory `isPoolSelected` (`1327–1328`); Physical Storage `isPoolSelected` (`1336–1337`); Networking `isPoolSelected` (`1339–1340`); GPU: `isPoolOrLiveStandaloneHost` = `isPoolSelected || (isHostSelected && isHostLive && selectionPool==null)` (`1354`) and `!FeatureForbidden(RestrictGpu)` (`1356–1358`). |
| **Host (live)** | General, Memory, Storage(Physical), Networking, NICs, GPU, USB, Console, Performance, Search | General `isHostSelected && (isHostLive || !is_connected)` (`1323–1325`); Memory `isHostSelected && isHostLive` (`1327–1328`); Physical Storage `isHostSelected && isHostLive` (`1336–1337`); Networking same (`1339–1340`); NICs `isHostSelected && isHostLive` (`1342–1343`); GPU (`1356–1358`); USB `isHostLive && PUSBs.Count>0 && !FeatureForbidden(RestrictUsbPassthrough)` (`1360–1361`); Console `isHostSelected && isHostLive` and no plugin console (`1372–1373`); Performance `isHostSelected && isHostLive` (`1378–1379`). |
| **Host (disconnected)** | General, Search | General via `!is_connected` branch (`1323–1325`); everything else requires `isHostLive`, so omitted. |
| **VM running** | General, Memory, Storage, Networking, Console, Performance, Snapshots, Search | `isVMSelected`+`isRealVMSelected` true: General (`1323–1325`), Memory (`1327–1328`), Storage `isRealVMSelected` (`1330–1331`), Networking `isVMSelected` (`1339–1340`), Console `isRealVMSelected` (`1372–1373`), Performance `isRealVMSelected` (`1378–1379`), Snapshots `isRealVMSelected` (`1381–1382`). |
| **VM halted** | General, Memory, Storage, Networking, Console, Performance, Snapshots, Search | Identical to “running”: `GetNewTabPages` never tests `power_state`, only `isRealVMSelected`/`isVMSelected`. |
| **Template (custom — no provision XML)** | General, Memory, Storage, Networking, Search | General/Memory/Networking via `isVMSelected`; Storage via `isTemplateSelected && !selectedTemplateHasProvisionXML` (`1330–1331`); no Console/Performance/Snapshots (those need `isRealVMSelected`). |
| **Default template (has provision XML)** | General, Memory, Networking, Search | Same as custom template but Storage omitted because `selectedTemplateHasProvisionXML` is true (`1330–1331`). |
| **Snapshot** | General, Memory, Networking, Search | `isVMSelected` true (`is_a_template`), but `isRealVMSelected` false and `isTemplateSelected` false (`is_a_snapshot`), so Storage/Console/Performance/Snapshots omitted. |
| **SR** | General, Storage(SR), Driver Domain Console, Search | General `isSRSelected` (`1323–1325`); Storage(SR) `isSRSelected` (`1333–1334`); Driver Domain Console `isSRSelected && selectedSr.HasDriverDomain(out _)` and no plugin console (`1375–1376`). |
| **Network** | Search | No flag matches `XenAPI.Network` in `GetNewTabPages`; only Search. |
| **VDI** | General, Search | General `isVdiSelected` (`1323–1325`); no other tab matches. |
| **Folder / tag / grouping (`GroupingTag`)** | Search | `First` is a `GroupingTag`, not an `IXenObject` (`SelectedItem.cs:198–208`), so every flag is false and `show_home` false. |
| **Multiple selection** | Search | `multi` true disables every other tab. |
| **Docker container** (extra row) | General, Processes, Details, Search (or General, Search on Stockholm+ server) | General `isDockerContainerSelected` (`1323–1325`); Processes + Details only if `!Helpers.StockholmOrGreater(connection)` (`1345–1347`), Processes only if `!Parent.IsWindows()` (`1348–1349`), Details always in that block (`1351`). |

**Plugin feature tabs** (`GetFeatureTabPages`, `MainWindow.cs:1394–1428`): for a single selection, each plugin `TabPageFeature` with `ShowTab` is added; console-replacement features (`IsConsoleReplacement`) go into `consoleFeatures` (added before `Console`, `1369–1370`) and suppress the built-in `Console`/`CvmConsole` pages (`1372,1375`), while non-console features go into `otherFeatures` (added after Snapshots, `1384–1385`).

---

# 2. Infrastructure / Objects / Organization tree building

`MainWindowTreeBuilder.cs` does **not** itself contain the Infrastructure grouping rules — it creates the root node and then delegates population to the `Search` grouping engine via `search.PopulateAdapters(...)`. The relevant pieces are:

## 2a. Root node and mode dispatch — `MainWindowTreeBuilder.cs`

- `CreateNewRootNode(search, mode)` (`MainWindowTreeBuilder.cs:131–184`) switches on `NavigationPane.NavigationMode`:
  - `Objects/Tags/Folders/CustomFields/vApps` → uses the corresponding `OrganizationView*` object’s `RootNode` and calls its `Populate(search, groupAcceptor)` (`138–162`).
  - `SavedSearch` → a node labeled with `search.Name` and a search icon (`163–174`).
  - **default** (Infrastructure **and** Notifications) → `new VirtualTreeNode(BrandManager.BrandConsole) { ImageIndex = (int)Icons.Home }` and `search.PopulateAdapters(groupAcceptor)` (`175–180`). This is the tree root node (the XenCenter/console brand name).
- The node-type mapping is in `MainWindowTreeNodeGroupAcceptor.Add(...)` (`319–372`): `Pool`→`AddPoolNode` (`392`), `Host`→`AddHostNode` (`420`), `VM`→`AddVMNode` (`397`), `DockerContainer`→`AddDockerContainerNode` (`410`), `VM_appliance`→`AddVmApplianceNode` (`415`), `SR`→`AddSRNode` (`425`), `XenAPI.Network`→`AddNetworkNode` (`432`), `VDI`→`AddVDINode` (`445`), `Folder`→`AddFolderNode` (`451`), anything else (tag value, custom-field value, `DateTime`, …) → generic node with a `GroupingTag` (`362–369`).
- `AddNode` inserts into the parent’s child list at a running index `_index` and increments it (`465–466`) — i.e. **insertion order is preserved; no sorting happens in this file** (sorting is done earlier by the grouping engine, see 2d).

## 2b. Infrastructure search and grouping definition

- The Infrastructure/Notifications search is chosen in `NavigationPane.Search`: `TreeSearch.DefaultTreeSearch ?? TreeSearch.SearchFor(null)` (`NavigationPane.cs:135–137`).
- `TreeSearch.SearchFor(null)` builds the scope via `GetTreeSearchScope()` (`TreeSearch.cs:49–70`), then calls `Search.SearchFor(null, scope)`.
- The Infrastructure grouping is created in the `else` branch of `Search.SearchFor(IXenObject value, QueryScope scope)` (`Search.cs:508–518`):

  ```
  Grouping dockervmGrouping = new XenModelObjectPropertyGrouping<VM>(PropertyNames.dockervm, null);        // Search.cs:512
  Grouping hostGrouping     = new XenModelObjectPropertyGrouping<Host>(PropertyNames.host, dockervmGrouping); // Search.cs:513
  Grouping poolGrouping     = new XenModelObjectPropertyGrouping<Pool>(PropertyNames.pool, hostGrouping);      // Search.cs:514
  return new Search(new Query(scope, null), poolGrouping, Messages.SEARCH_TITLE_OVERVIEW, null, false);          // Search.cs:516–517
  ```

  So the Infrastructure hierarchy is **Root → Pool → Host → VM → (Docker container)**, produced by the three-level `pool` → `host` → `dockervm` grouping.

## 2c. Which objects appear under which parent

The group of each object is computed by the property accessors (`Common.cs`):

- **Pool level** — `PropertyNames.pool` = `o => Helpers.GetPool(o.Connection)` (`Common.cs:312`). Every object whose connection has a pool is placed under that pool; objects on a connection **without** a pool (standalone host, disconnected host) are “ungrouped” and appear directly under the root (`GroupAlg.cs` `NodeGroup.Add`/`AddUngrouped`, `415–418`, `515–525`). A disconnected connection is faked as a `Host` with no pool (`GroupAlg.cs:92–102`).
- **Host level** — `PropertyNames.host` = `HostProperty` (`Common.cs:313`, defined `745–794`):
  - no pool → all hosts of the connection (`754–757`);
  - `VM` → `vm.Home()` (`758–763`);
  - `SR` → `sr.Home()` (`764–769`);
  - `Network` with no PIFs → all hosts (`770–774`);
  - `Host` → itself (`775–778`);
  - `VDI` → its SR’s `Home()` (`779–784`);
  - `DockerContainer` → parent VM’s `Home()` (`785–791`).
  - Empty result → object is ungrouped, i.e. appears directly under the pool.
- **VM level (3rd)** — `PropertyNames.dockervm` = Docker container’s parent VM (`Common.cs:315`); non-docker objects return an empty list, so only Docker containers get a VM parent.
- `VM.Home()` (`VM.cs:457–479`): snapshot → source VM’s home (`459–463`); template → **null** (`465–466`); running/paused → `resident_on` (`468–469`); else storage host, else live affinity host, else null (`471–479`). ⇒ **templates have no host and sit at the pool level**; running VMs sit under `resident_on`; halted VMs under storage/affinity host (or pool if none).
- `SR.Home()` (`SR.cs:82–92`): returns null if `shared || PBDs.Count != 1`; otherwise the single PBD’s host. ⇒ **shared/remote SRs sit at the pool level; local SRs sit under their one host.**

**Which object types are in scope at all** — the Infrastructure tree is a `QueryScope`, and membership is decided by `PropertyNames.type` (`TypeProperty`, `Common.cs:610–651`) matched against the scope (`QueryScope.WantType`, `QueryScope.cs:71–82`):

- Always included: `DisconnectedServer | Server | VM | RemoteSR | DockerContainer` (`Search.DefaultObjectTypes()`, `Search.cs:521–527`) plus `Pool` (added unconditionally, `TreeSearch.cs:58`).
- Conditionally included (see View toggles below): `DefaultTemplate`, `UserTemplate`, `LocalSR` (`TreeSearch.cs:60–67`).
- **Not** in the Infrastructure scope: `Snapshot`, `VDI`, `Network`, `Folder`, `Appliance` (they appear in the Objects view or the dedicated default searches instead).

## 2d. Sort order

`GroupAlg.cs`:

- Groups are sorted by `CompareGroupKeys` before emission (`GroupAlg.cs:343–349`), and leaf items by `items.Sort(Compare)` (`570–574`).
- `Compare` rules (`GroupAlg.cs:181–243`), in order:
  1. non-`IXenObject` keys (tag values, dates, custom-field values) sort **before** `IXenObject`s (`193–220`);
  2. `search.Sorting` if present (`197–205`, `224–232`);
  3. `CompareByType` → `TypeOf`: `Folder`="10", `Pool`="20", `Host`="30", real VM="40", otherwise the type name string (`251–270`);
  4. the type’s own `Comparer.Default` (`238–240`);
  5. `StringUtility.NaturalCompare` of names (`242`).
- Hidden items are filtered out in `Hide` (`GroupAlg.cs:106–147`): provider-hidden objects (`108`), control-domain VMs and hidden VMs/SRs/networks when `ShowHiddenVMs` is off (`118,128,138`), VMs/SRs whose home host isn’t live (`121–134`), the tools SR (`128`), and the root folder (`140–143`).

## 2e. View-menu toggles (hide/show nodes)

Settings flags read by `TreeSearch.GetTreeSearchScope()` (`TreeSearch.cs:60–67`), driven by the View menu (`MainWindow.cs`):

| View menu item | Setting | Effect | Source |
|---|---|---|---|
| “{0} Templates” | `DefaultTemplatesVisible` | show/hide **default templates** (`ObjectTypes.DefaultTemplate`) | `MainWindow.cs:1590,1950–1955`; `TreeSearch.cs:60–61` |
| “Custom Templates” | `UserTemplatesVisible` | show/hide **custom templates** (`ObjectTypes.UserTemplate`) | `MainWindow.cs:1591,1957–1962`; `TreeSearch.cs:63–64` |
| “Local Storage” | `LocalSRsVisible` | show/hide **local SRs** (`ObjectTypes.LocalSR`) | `MainWindow.cs:1592,1964–1969`; `TreeSearch.cs:66–67` |
| “Hidden Objects” | `ShowHiddenVMs` | show/hide hidden VMs/SRs/networks (used in `Hide`) | `MainWindow.cs:1593,1971–1976`; `GroupAlg.cs:118,128,138` |

All four call `ViewSettingsChanged()` → `navigationPane.UpdateSearch()` + `RequestRefreshTreeView()` (`MainWindow.cs:1978–1983`).

## 2f. Objects and Organization views grouping

The search/grouping for each mode is selected in `NavigationPane.Search` (`NavigationPane.cs:128–151`) and then wrapped by the `OrganizationView*` classes (`OrganizationalView.cs`), which set the root node and the grouping/filter (`OrgViewSearch`, `80–107`):

| Mode | Search (`NavigationPane.cs`) | Root name | Grouping | Filter | Source (`OrganizationalView.cs`) |
|---|---|---|---|---|---|
| **Objects** | `Search.SearchForAllTypes()` (scope `AllExcFolders`) | “Objects” (`Messages.VIEW_OBJECTS`) | by **type** `PropertyGrouping<ObjectTypes>(PropertyNames.type)` | none | `111–133` |
| **Tags** | `Search.SearchForTags()` | “Tags” | by **tag** `PropertyGrouping<String>(PropertyNames.tags)` | has ≥1 tag (`ListEmptyQuery<String>(tags,false)`) | `136–164` |
| **Folders** | `Search.SearchForFolders()` | “Folders” | `FolderGrouping` (folder hierarchy) | has a folder (`NullQuery<Folder>(folder,false)`); `IncludeFolders=true` | `198–228` |
| **Custom Fields** | `Search.SearchForCustomFields()` | “Custom Fields” | `AllCustomFieldsGrouping` (field key → value) | has custom fields (`BooleanQuery(has_custom_fields,true)`) | `167–195` |
| **vApps** | `Search.SearchForVapps()` | “vApps” (`Messages.VM_APPLIANCES`) | by `VM_appliance` (`XenModelObjectPropertyGrouping<VM_appliance>(PropertyNames.appliance)`) | in any appliance (`BooleanQuery(in_any_appliance,true)`) | `231–259` |

The group-acceptor node mapping (2a) is what turns those group keys (type names, tag strings, folders, custom-field values, appliances) into the actual tree nodes.

---

# 3. Navigation modes, buttons and Notifications — `NavigationPane.cs` / `NavigationView.cs`

## 3a. Navigation modes

`NavigationPane.NavigationMode` enum (`NavigationPane.cs:46–50`):

```
Infrastructure, Objects, Tags, Folders, CustomFields, vApps, SavedSearch, Notifications
```

`NavigationView.cs` holds the mode in `NavigationMode` (`NavigationView.cs:114`) and rebuilds the tree in `RefreshTreeView` via `treeBuilder.CreateNewRootNode(CurrentSearch.AddFullTextFilter(searchTextBox.Text), NavigationMode)` (`NavigationView.cs:357–375`).

## 3b. Buttons — labels and order

There are two tool strips: a big (vertical, full labels) and a small (horizontal, icon-only) one; each big button is paired with a small one (`NavigationPane.cs:92–96`).

**Big strip order** (`NavigationPane.Designer.cs:98–103`) and labels (`NavigationPane.resx`):

| # | Button | Label | resx |
|---|---|---|---|
| 1 | `buttonInfraBig` | Infrastructure | 219–220 |
| 2 | `buttonObjectsBig` | Objects | 237–238 |
| 3 | `buttonOrganizationBig` (dropdown) | Organization Views | 279–280 |
| 4 | `buttonSearchesBig` (dropdown) | Saved Searches | 297–298 |
| 5 | `buttonNotifyBig` | Notifications | 315–316 |

Mode tags assigned in code: Infrastructure→`Infrastructure`, Objects→`Objects`, Notify→`Notifications` (`NavigationPane.cs:98–100`); the Organization dropdown items are tagged `CustomFields/Folders/Tags/vApps` (`242–251`); the Searches dropdown is populated from `Search.Searches` sorted by name, `DefaultSearch` items getting a different icon (`253–275`).

The **small strip** lists the same five in reverse (`NavigationPane.Designer.cs:184–189`) but they are all `ToolStripItemAlignment.Right` (`195,207,217,227,237`), so visually they still appear left-to-right as Infrastructure → Objects → Organization Views → Saved Searches → Notifications.

## 3c. Organization Views sub-items

Order and labels (`NavigationPane.Designer.cs:126–130`, `NavigationPane.resx`):

| # | Item | Label | Mode (tag) |
|---|---|---|---|
| 1 | `toolStripMenuItemTags` | Objects by Tag | `Tags` |
| 2 | `toolStripMenuItemFolders` | Objects by Folder | `Folders` |
| 3 | `toolStripMenuItemFields` | Objects by Custom Field | `CustomFields` |
| 4 | `toolStripMenuItemVapps` | vApps | `vApps` |

Tags are set in `PopulateOrganizationDropDown()` (`NavigationPane.cs:242–251`).

## 3d. Notifications sub-items and counts

- Sub-mode enum: `Alerts, Updates, UpdatesFromCdn, Events` (`NotificationsView.cs:147`).
- The `NotificationsView` list is populated in its constructor with **only two items**: **Alerts** and **Events** (`NotificationsView.cs:50–51`). (No `NotificationsSubModeItem` for `Updates`/`UpdatesFromCdn` is ever constructed elsewhere — `new NotificationsSubModeItem` appears only at `NotificationsView.cs:50–51`. Those two enum values are still reachable from the status-bar buttons via `SwitchToNotificationsView`, but no list item corresponds to them.)
- Counts: `UnreadEntries` — for **Alerts and Updates** this is the total number of entries; for **Events** it is the number of *error* entries (`NotificationsView.cs:158–162`).
- Display text `GetText` (`NotificationsView.cs:189–211`), using `Messages.resx` (10115–10134):
  - Alerts: `Alerts` (0) → `Alerts ({0})`
  - Updates / UpdatesFromCdn: `Updates` (0) → `Updates ({0})`
  - Events: `Events` (0) → `Events (1 error)` → `Events ({0} errors)`
- Events are drawn **bold** when `UnreadEntries > 0` (`NotificationsView.cs:118–120`).
- Icons `GetImage` (`NotificationsView.cs:170–187`): Events use an “errors” icon when unread > 0; Alerts/Updates use fixed icons.
- The **Notifications button badge** shows the **sum** of all sub-mode unread entries: `GetTotalEntries()` sums `UnreadEntries` (`NotificationsView.cs:54–63`), and `UpdateNotificationsButton(mode, entries)` sets the button count to that total and updates the matching sub-mode (`NavigationPane.cs:167–171`).
- Counts are pushed from `MainWindow.cs`: Events from `errorCount` (`MainWindow.cs:536`), Alerts from an alert count (`MainWindow.cs:2636`).
- Status-bar buttons switch to a sub-mode: Alerts (`MainWindow.cs:2906–2909`), Updates (`2911–2914`), CDN Updates (`2916–2919`), Events/errors (`2921–2929`).
- Selecting a sub-mode raises `NotificationsSubModeChanged`, which remembers `lastNotificationsMode` and re-opens that view on the next visit (`NavigationPane.cs:397–403`, `52`, `284`).
