import { useEffect, useMemo, useState } from "react"
import { Activity, ArrowLeft, ChevronLeft, ChevronRight, Cpu, Database, HardDrive, Info, Loader2, LockKeyhole, MemoryStick, Network, RefreshCw, Search, Server, ShieldCheck, TriangleAlert } from "lucide-react"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { errorMessage } from "@/lib/api"
import { hostApi, type DataSection, type DetailRecord, type HostDetailsData, type HostPerformance, type InfoGroup, type PerformanceRange, type PerformanceSeries } from "@/lib/host-api"
import "./host-details.css"

type HostDetailsProps = {
  connectionId: string; hostUuid: string; hostName: string; connectionName?: string; readOnly: boolean; onBack: () => void
}
type LoadState<T> = { status: "loading" } | { status: "error"; message: string } | { status: "ready"; data: T }

function timestamp(value: number): string {
  return new Date(value * 1000).toLocaleString()
}

function Loading({ label }: { label: string }) {
  return <div className="hd-state" role="status"><Loader2 className="animate-spin" size={23} /><strong>{label}</strong><span>Reading information from the selected host.</span></div>
}

function Failed({ message, retry }: { message: string; retry: () => void }) {
  return <div className="hd-state hd-error" role="alert"><TriangleAlert size={24} /><strong>Could not load host information</strong><p>{message}</p><Button variant="outline" onClick={retry}><RefreshCw size={14} />Retry</Button></div>
}

function InformationGroups({ groups }: { groups: InfoGroup[] }) {
  if (!groups.length) return <div className="hd-state hd-compact"><Info size={21} /><span>No information was returned for this section.</span></div>
  return <div className="hd-info-grid">{groups.map((group, index) => <section className="hd-info-group" key={`${group.title}-${index}`}>
    <h3>{group.title}</h3><dl>{group.fields.map((field, fieldIndex) => <div key={`${field.label}-${fieldIndex}`}><dt>{field.label}</dt><dd>{field.value || "—"}</dd></div>)}</dl>
  </section>)}</div>
}

function SectionStatus({ section }: { section: Exclude<DataSection, { status: "ready" }> }) {
  return <div className={`hd-state hd-compact hd-${section.status}`}><Info size={22} /><strong>{section.status === "unsupported" ? "Not supported by this host" : "Information unavailable"}</strong><p>{section.message}</p></div>
}

function RecordSection({ title, subtitle, section }: { title: string; subtitle?: string; section: DataSection }) {
  const [query, setQuery] = useState("")
  const [page, setPage] = useState(0)
  const [selected, setSelected] = useState<DetailRecord | null>(null)
  const rows = section.status === "ready" ? section.rows : []
  const needle = query.trim().toLocaleLowerCase()
  const filtered = rows.filter(row => !needle || [row.title, row.id, ...Object.values(row.values), ...row.groups.flatMap(group => [group.title, ...group.fields.flatMap(field => [field.label, field.value])])].some(value => value.toLocaleLowerCase().includes(needle)))
  const pageCount = Math.max(1, Math.ceil(filtered.length / 25))
  const currentPage = Math.min(page, pageCount - 1)
  const visible = filtered.slice(currentPage * 25, (currentPage + 1) * 25)
  const columns = section.status === "ready" ? section.columns : []
  return <section className="hd-record-section">
    <div className="hd-section-heading"><div><h3>{title}</h3>{subtitle && <p>{subtitle}</p>}</div>{section.status === "ready" && <Badge variant="secondary">{rows.length.toLocaleString()} {rows.length === 1 ? "record" : "records"}</Badge>}</div>
    {section.status !== "ready" ? <SectionStatus section={section} /> : rows.length === 0 ? <div className="hd-state hd-compact"><Database size={22} /><strong>No {title.toLocaleLowerCase()} reported</strong><span>The host returned an empty list.</span></div> : <>
      <div className="hd-table-tools"><div className="hd-search"><Search size={15} /><Input aria-label={`Search ${title}`} value={query} placeholder={`Search ${title.toLocaleLowerCase()}…`} onChange={event => { setQuery(event.target.value); setPage(0) }} /></div><span>{filtered.length === rows.length ? "Select a record for full details" : `${filtered.length} of ${rows.length} records`}</span></div>
      <Table className="hd-table"><TableHeader><TableRow>{columns.length ? columns.map(column => <TableHead key={column}>{column}</TableHead>) : <TableHead>Name</TableHead>}<TableHead className="hd-details-column"><span className="sr-only">Details</span></TableHead></TableRow></TableHeader><TableBody>{visible.map(row => <TableRow key={row.id} onClick={() => setSelected(row)} className="hd-clickable-row">
        {columns.length ? columns.map((column, index) => <TableCell key={column}>{index === 0 ? <button className="hd-record-link" onClick={event => { event.stopPropagation(); setSelected(row) }}>{row.values[column] || row.title || "—"}</button> : row.values[column] || "—"}</TableCell>) : <TableCell><button className="hd-record-link" onClick={event => { event.stopPropagation(); setSelected(row) }}>{row.title}</button></TableCell>}
        <TableCell className="hd-details-column"><Button variant="ghost" size="icon-sm" aria-label={`View details for ${row.title}`} onClick={event => { event.stopPropagation(); setSelected(row) }}><ChevronRight size={15} /></Button></TableCell>
      </TableRow>)}{!visible.length && <TableRow><TableCell colSpan={Math.max(columns.length, 1) + 1}><div className="hd-no-matches">No records match “{query}”.</div></TableCell></TableRow>}</TableBody></Table>
      <div className="hd-pagination"><span>{filtered.length ? `${currentPage * 25 + 1}–${Math.min((currentPage + 1) * 25, filtered.length)} of ${filtered.length}` : "0 records"}</span><div><Button size="sm" variant="outline" disabled={currentPage === 0} onClick={() => setPage(currentPage - 1)} aria-label={`Previous ${title} page`}><ChevronLeft size={14} /></Button><span>Page {currentPage + 1} of {pageCount}</span><Button size="sm" variant="outline" disabled={currentPage + 1 >= pageCount} onClick={() => setPage(currentPage + 1)} aria-label={`Next ${title} page`}><ChevronRight size={14} /></Button></div></div>
    </>}
    <Dialog open={selected !== null} onOpenChange={open => { if (!open) setSelected(null) }}><DialogContent className="hd-record-dialog"><DialogHeader><DialogTitle>{selected?.title}</DialogTitle><DialogDescription>{title} · {selected?.id}</DialogDescription></DialogHeader>{selected && <InformationGroups groups={[{ title: "Summary", fields: Object.entries(selected.values).map(([label, value]) => ({ label, value })) }, ...selected.groups]} />}</DialogContent></Dialog>
  </section>
}

function metricValue(value: number, unit: string): string {
  return `${value.toLocaleString(undefined, { maximumFractionDigits: Math.abs(value) < 1 ? 4 : 2 })}${unit ? ` ${unit}` : ""}`
}

function MetricChart({ series, start, end, step }: { series: PerformanceSeries; start: number; end: number; step: number }) {
  const [hover, setHover] = useState<number | null>(null)
  const points = useMemo(() => [...series.points].sort((left, right) => left.timestamp - right.timestamp), [series.points])
  const values = points.flatMap(point => point.value === null ? [] : [point.value])
  const low = Math.min(0, ...values)
  const high = Math.max(low + 1, ...values)
  const width = 660, height = 160, left = 70, right = 15, top = 16, bottom = 26
  const x = (time: number) => left + ((time - start) / Math.max(1, end - start)) * (width - left - right)
  const y = (value: number) => top + (1 - (value - low) / (high - low)) * (height - top - bottom)
  let path = "", hasPrevious = false, previousTime = 0
  for (const point of points) {
    if (point.value === null) { hasPrevious = false; continue }
    const contiguous = hasPrevious && (step <= 0 || point.timestamp - previousTime <= step * 1.5)
    path += `${contiguous ? "L" : "M"}${x(point.timestamp).toFixed(2)},${y(point.value).toFixed(2)} `
    hasPrevious = true; previousTime = point.timestamp
  }
  const latest = points.at(-1)
  const hovered = hover === null ? undefined : points.reduce<PerformanceSeries["points"][number] | undefined>((best, point) => !best || Math.abs(point.timestamp - hover) < Math.abs(best.timestamp - hover) ? point : best, undefined)
  const displayed = hovered ?? latest
  return <section className="hd-metric"><div className="hd-metric-heading"><h4>{series.name}</h4><strong>{displayed?.value !== undefined && displayed.value !== null ? metricValue(displayed.value, series.unit) : "No sample"}</strong></div>
    <div className="hd-metric-context">{displayed ? `${hovered ? "Sample" : "Latest sample"} · ${timestamp(displayed.timestamp)}` : "No samples in this time range"}</div>
    {values.length ? <svg viewBox={`0 0 ${width} ${height}`} role="img" aria-label={`${series.name}, ${series.unit || "value"}, from ${timestamp(start)} to ${timestamp(end)}`} className="hd-chart" onMouseLeave={() => setHover(null)} onMouseMove={event => { const bounds = event.currentTarget.getBoundingClientRect(); const position = (event.clientX - bounds.left) / bounds.width * width; setHover(start + Math.max(0, Math.min(1, (position - left) / (width - left - right))) * (end - start)) }}>
      {[0, 0.5, 1].map(fraction => { const value = low + fraction * (high - low); return <g key={fraction}><line x1={left} y1={y(value)} x2={width - right} y2={y(value)} className="hd-chart-grid" /><text x={left - 8} y={y(value) + 3} textAnchor="end">{value.toLocaleString(undefined, { maximumFractionDigits: 2, notation: "compact" })}</text></g> })}
      <path d={path} className="hd-chart-line" />{points.filter(point => point.value !== null && (points.length === 1 || values.length === 1)).map(point => point.value === null ? null : <circle key={point.timestamp} cx={x(point.timestamp)} cy={y(point.value)} r={3} className="hd-chart-dot" />)}
      {hovered && <g><line x1={x(hovered.timestamp)} y1={top} x2={x(hovered.timestamp)} y2={height - bottom} className="hd-chart-cursor" />{hovered.value !== null && <circle cx={x(hovered.timestamp)} cy={y(hovered.value)} r={3.5} className="hd-chart-dot" />}</g>}
      <text x={left} y={height - 6}>{new Date(start * 1000).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}</text><text x={width - right} y={height - 6} textAnchor="end">{new Date(end * 1000).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}</text>
    </svg> : <div className="hd-chart-empty">No numeric samples available. Missing values are not plotted.</div>}
  </section>
}

function Performance({ connectionId, hostUuid }: Pick<HostDetailsProps, "connectionId" | "hostUuid">) {
  const [range, setRange] = useState<PerformanceRange>(900)
  const [revision, setRevision] = useState(0)
  const [state, setState] = useState<LoadState<HostPerformance>>({ status: "loading" })
  useEffect(() => {
    const controller = new AbortController()
    let active = true
    setState({ status: "loading" })
    void hostApi.performance({ connectionId, hostUuid, rangeSeconds: range, signal: controller.signal }).then(data => { if (active) setState({ status: "ready", data }) }).catch(error => { if (active) setState({ status: "error", message: errorMessage(error) }) })
    return () => { active = false; controller.abort() }
  }, [connectionId, hostUuid, range, revision])
  return <div className="hd-performance"><div className="hd-performance-toolbar"><div><h3>Host performance</h3><p>Host RRD samples · refresh manually · gaps indicate missing data</p></div><div className="hd-range" aria-label="Performance time range">{([{ seconds: 900, label: "15 min" }, { seconds: 3600, label: "1 hour" }, { seconds: 86400, label: "24 hours" }] satisfies { seconds: PerformanceRange; label: string }[]).map(option => <Button key={option.seconds} variant={range === option.seconds ? "secondary" : "ghost"} size="sm" aria-pressed={range === option.seconds} onClick={() => setRange(option.seconds)}>{option.label}</Button>)}</div><Button size="sm" variant="outline" disabled={state.status === "loading"} onClick={() => setRevision(value => value + 1)}><RefreshCw size={14} />Refresh</Button></div>
    {state.status === "loading" ? <Loading label="Loading performance samples" /> : state.status === "error" ? <Failed message={state.message} retry={() => setRevision(value => value + 1)} /> : state.data.status !== "ready" ? <SectionStatus section={state.data} /> : <PerformanceMetrics data={state.data} />}
  </div>
}

function PerformanceMetrics({ data }: { data: Extract<HostPerformance, { status: "ready" }> }) {
  const [query, setQuery] = useState("")
  const [page, setPage] = useState(0)
  const series = data.series.filter(metric => metric.name.toLocaleLowerCase().includes(query.trim().toLocaleLowerCase()))
  const pageCount = Math.max(1, Math.ceil(series.length / 12))
  const currentPage = Math.min(page, pageCount - 1)
  return <>
    <div className="hd-performance-meta"><span>{timestamp(data.start)} — {timestamp(data.end)}</span><span>{data.step}s sample interval · {data.series.length} metrics</span></div>
    {data.series.length ? <>
      <div className="hd-search hd-metric-search"><Search size={15} /><Input placeholder="Search performance metrics…" aria-label="Search performance metrics" value={query} onChange={event => { setQuery(event.target.value); setPage(0) }} /></div>
      <div className="hd-metric-grid">{series.slice(currentPage * 12, (currentPage + 1) * 12).map(metric => <MetricChart key={metric.name} series={metric} start={data.start} end={data.end} step={data.step} />)}</div>
      {!series.length && <div className="hd-state hd-compact">No metrics match “{query}”.</div>}
      <div className="hd-pagination"><span>{series.length ? `${currentPage * 12 + 1}–${Math.min((currentPage + 1) * 12, series.length)} of ${series.length} metrics` : "0 metrics"}</span><div><Button variant="outline" size="sm" disabled={currentPage === 0} onClick={() => setPage(currentPage - 1)} aria-label="Previous metrics page"><ChevronLeft size={14} /></Button><span>Page {currentPage + 1} of {pageCount}</span><Button variant="outline" size="sm" disabled={currentPage + 1 >= pageCount} onClick={() => setPage(currentPage + 1)} aria-label="Next metrics page"><ChevronRight size={14} /></Button></div></div>
    </> : <div className="hd-state hd-compact"><Activity size={22} /><strong>No performance metrics reported</strong><span>The host returned no series for this time range.</span></div>}
  </>
}

function HostDetailsView({ connectionId, hostUuid, hostName, connectionName, readOnly, onBack }: HostDetailsProps) {
  const [state, setState] = useState<LoadState<HostDetailsData>>({ status: "loading" })
  const [revision, setRevision] = useState(0)
  const [tab, setTab] = useState("general")
  useEffect(() => {
    const controller = new AbortController()
    let active = true
    setState({ status: "loading" })
    void hostApi.details({ connectionId, hostUuid, signal: controller.signal }).then(data => { if (active) setState({ status: "ready", data }) }).catch(error => { if (active) setState({ status: "error", message: errorMessage(error) }) })
    return () => { active = false; controller.abort() }
  }, [connectionId, hostUuid, revision])
  const data = state.status === "ready" ? state.data : null
  return <div className="host-details"><div className="hd-back"><Button variant="ghost" size="sm" onClick={onBack}><ArrowLeft size={14} />Back to hosts</Button><span>{connectionName || connectionId}</span></div>
    <header className="hd-header"><div className="hd-host-icon"><Server size={27} /></div><div className="hd-host-identity"><div className="hd-eyebrow">HOST DETAILS</div><h1>{data?.name || hostName}</h1><p>{data?.address && <span>{data.address} · </span>}<code>{hostUuid}</code></p></div><div className="hd-header-actions"><Badge variant="outline" className="hd-readonly"><LockKeyhole size={12} />{readOnly ? "Read-only connection" : "Read-only host view"}</Badge><Button variant="outline" size="sm" onClick={() => setRevision(value => value + 1)} disabled={state.status === "loading"}><RefreshCw size={14} />Refresh details</Button></div></header>
    {data && <div className="hd-collected"><ShieldCheck size={13} /><span>Host information · {connectionName || connectionId}</span><span>Collected {timestamp(data.collected_at)}</span></div>}
    {state.status === "loading" ? <Loading label="Loading host details" /> : state.status === "error" ? <Failed message={state.message} retry={() => setRevision(value => value + 1)} /> : <>
      {state.data.warnings.length > 0 && <details className="hd-warnings"><summary><TriangleAlert size={15} />{state.data.warnings.length} collection {state.data.warnings.length === 1 ? "notice" : "notices"}<span>Some host information may be incomplete</span></summary><ul>{state.data.warnings.map((warning, index) => <li key={index}><strong>{warning.section}</strong> — {warning.message}</li>)}</ul></details>}
      <Tabs value={tab} onValueChange={setTab} className="hd-tabs"><TabsList variant="line" aria-label="Host information sections"><TabsTrigger value="general"><Info size={14} />General</TabsTrigger><TabsTrigger value="cpu"><Cpu size={14} />CPU & memory</TabsTrigger><TabsTrigger value="performance"><Activity size={14} />Performance</TabsTrigger><TabsTrigger value="networking"><Network size={14} />Networking</TabsTrigger><TabsTrigger value="storage"><HardDrive size={14} />Storage</TabsTrigger><TabsTrigger value="hardware"><MemoryStick size={14} />Hardware</TabsTrigger><TabsTrigger value="updates">Updates</TabsTrigger><TabsTrigger value="certificates">Certificates</TabsTrigger><TabsTrigger value="health">Health</TabsTrigger></TabsList>
        <TabsContent value="general"><InformationGroups groups={state.data.general} /></TabsContent>
        <TabsContent value="cpu"><div className="hd-tab-intro"><h2>CPU & memory</h2><p>Processor topology, host memory and available hardware information.</p></div><InformationGroups groups={state.data.memory} /><RecordSection title="Physical CPUs" section={state.data.cpu} /></TabsContent>
        <TabsContent value="performance">{tab === "performance" && <Performance connectionId={connectionId} hostUuid={hostUuid} />}</TabsContent>
        <TabsContent value="networking"><RecordSection title="Network interfaces" subtitle="Physical NICs, bonds, VLANs and management networking reported by the host." section={state.data.nics} /></TabsContent>
        <TabsContent value="storage"><RecordSection title="Storage repositories" subtitle="Host storage attachment and repository capacity." section={state.data.storage} /></TabsContent>
        <TabsContent value="hardware"><RecordSection title="GPUs" section={state.data.gpus} /><RecordSection title="PCI devices" section={state.data.pci} /><RecordSection title="USB devices" section={state.data.usb} /></TabsContent>
        <TabsContent value="updates"><RecordSection title="Updates & patches" subtitle="Installed and available update information exposed by the host API." section={state.data.updates} /></TabsContent>
        <TabsContent value="certificates"><RecordSection title="Certificates" section={state.data.certificates} /></TabsContent>
        <TabsContent value="health"><RecordSection title="Alerts" section={state.data.alerts} /><RecordSection title="Crash dumps" section={state.data.crashdumps} /></TabsContent>
      </Tabs>
    </>}
  </div>
}

export default function HostDetails(props: HostDetailsProps) {
  return <HostDetailsView key={`${props.connectionId}:${props.hostUuid}`} {...props} />
}
