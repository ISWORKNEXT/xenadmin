# Unit M7 — Monitoring: Performance graphs, alerts, live metrics

Read `docs/supervision/CHARTER.md` first. It is binding. Backend changes
are allowed.

## Reference (`/home/tinnerkun/Desktop/xen/upstream-xenadmin/XenAdmin/`)

- `TabPages/PerformancePage.cs`, `Controls/CustomDataGraph/` (GraphList,
  DataPlot, DataPlotNav, DataEventList, ArchiveMaintainer — RRD fetch via
  `/rrd_updates` and `/host_rrd`/`/vm_rrd`, DataKey/Palette for series
  names and colours), `Dialogs/GraphDetailsDialog.cs` (New/Edit Graph:
  graph name, datasource checklist with colours).
- `SettingsPanels/PerfmonAlertEditPage.cs`, `PerfmonAlertOptionsPage.cs`
  (CPU / network / disk / memory / SR alert thresholds stored in
  `other_config.perfmon` XML; mail options in pool `other_config`).
- `Dialogs/` + `Commands/ExportResourceReportCommand.cs` (Pool > Export
  Resource Data…, `XenModel/Actions/` resource report).

## Deliver

1. **Performance tab** (VM and host): graph list per upstream defaults
   (VM: CPU, Memory, Network, Disk; host: CPU, Memory, Network, Disk,
   Load average), time range zoom presets (Last 10 minutes, 2 hours,
   1 week, 1 year) and the DataPlotNav overview strip with a draggable
   window; hover shows values; data from `/rrd_updates` with archive
   step per range, refreshed every 5 s on the short range.
2. **Actions ▾** (from S3a): New Graph…, Edit Graph…, Delete Graph,
   Restore Default Graphs, Move Up / Move Down. GraphDetailsDialog lists
   every datasource the object exposes (`VM.get_data_sources` /
   `host.get_data_sources`), with colour picker. Graph layout persisted per
   object in pool `gui_config` with upstream key format so XCP-ng Center
   sees the same graphs.
3. **Live metrics elsewhere** (gaps from S3a/S3b): VM Memory tab shows
   used memory (guest metrics / RRD `memory_internal_free`), Search tab
   columns CPU Usage, Used Memory, Network, Disks, Address (guest
   metrics networks), Uptime fill from RRD/guest metrics with a light
   refresh (not per-frame).
4. **Performance alerts**: Properties > Alerts page (VM: CPU, network,
   disk; host: CPU, network, memory free, dom0 memory; SR: storage
   throughput) with threshold and duration, written to
   `other_config.perfmon` in upstream XML; pool Properties > Alert mail
   options page (SMTP server/port/address in pool `other_config`).
5. **Pool > Export Resource Data…**: export the upstream resource report
   as CSV (and XLS if trivial; else CSV only) with the upstream columns.

## Accept

- fmt, clippy, tests (RRD XML parse, perfmon XML encode/decode, gui_config
  graph layout round trip).
- Screenshots in `docs/supervision/evidence/M7/`: host Performance tab with
  4+ graphs, Graph details dialog, VM Alerts page, Search tab with CPU and
  Uptime filled.
- Live on .170: graphs show non-zero host CPU; create graph
  `zz-parity-M7-graph` on the host, readback in `gui_config`, delete it;
  set a CPU alert on a `zz-parity-M7-vm`, readback `other_config.perfmon`,
  remove; export resource data CSV and check it lists the host. Clean up.
