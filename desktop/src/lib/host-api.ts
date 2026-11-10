import { invoke, isTauri } from "@tauri-apps/api/core"
import { z } from "zod"

const infoFieldSchema = z.object({ label: z.string(), value: z.string() })
const infoGroupSchema = z.object({ title: z.string(), fields: z.array(infoFieldSchema) })
const detailRecordSchema = z.object({
  id: z.string(), title: z.string(), values: z.record(z.string(), z.string()), groups: z.array(infoGroupSchema),
})
const dataSectionSchema = z.discriminatedUnion("status", [
  z.object({ status: z.literal("ready"), columns: z.array(z.string()), rows: z.array(detailRecordSchema) }),
  z.object({ status: z.literal("unsupported"), message: z.string() }),
  z.object({ status: z.literal("unavailable"), message: z.string() }),
])
const hostDetailsSchema = z.object({
  host_uuid: z.string(), name: z.string(), address: z.string(), collected_at: z.number(),
  general: z.array(infoGroupSchema), memory: z.array(infoGroupSchema),
  cpu: dataSectionSchema, nics: dataSectionSchema, storage: dataSectionSchema,
  gpus: dataSectionSchema, pci: dataSectionSchema, usb: dataSectionSchema,
  updates: dataSectionSchema, certificates: dataSectionSchema, alerts: dataSectionSchema, crashdumps: dataSectionSchema,
  warnings: z.array(z.object({ section: z.string(), message: z.string() })),
})
const performanceSchema = z.discriminatedUnion("status", [
  z.object({
    status: z.literal("ready"), start: z.number(), end: z.number(), step: z.number(),
    series: z.array(z.object({
      name: z.string(), unit: z.string(),
      points: z.array(z.object({ timestamp: z.number(), value: z.number().nullable() })),
    })),
  }),
  z.object({ status: z.literal("unsupported"), message: z.string() }),
  z.object({ status: z.literal("unavailable"), message: z.string() }),
])

export type InfoGroup = z.infer<typeof infoGroupSchema>
export type DetailRecord = z.infer<typeof detailRecordSchema>
export type DataSection = z.infer<typeof dataSectionSchema>
export type HostDetailsData = z.infer<typeof hostDetailsSchema>
export type HostPerformance = z.infer<typeof performanceSchema>
export type PerformanceSeries = Extract<HostPerformance, { status: "ready" }>["series"][number]
export type PerformanceRange = 900 | 3600 | 86400

async function readCommand(name: "host_details" | "host_performance", args: Record<string, unknown>, signal?: AbortSignal): Promise<unknown> {
  if (isTauri()) return invoke<unknown>(name, args)
  const response = await fetch("/api/command", {
    method: "POST", headers: { "Content-Type": "application/json", "X-Xen-Desktop-Preview": "1" },
    body: JSON.stringify({ command: name, args }), signal,
  })
  if (!response.headers.get("content-type")?.includes("application/json")) {
    throw new Error("The desktop preview backend is not running. Open the native app or start npm run preview:desktop.")
  }
  const result: unknown = await response.json()
  if (!response.ok) {
    const parsed = z.object({ error: z.string() }).safeParse(result)
    throw new Error(parsed.success ? parsed.data.error : "The host information could not be retrieved.")
  }
  return result
}

export const hostApi = {
  async details({ connectionId, hostUuid, signal }: { connectionId: string; hostUuid: string; signal?: AbortSignal }): Promise<HostDetailsData> {
    return hostDetailsSchema.parse(await readCommand("host_details", { connectionId, hostUuid }, signal))
  },
  async performance({ connectionId, hostUuid, rangeSeconds, signal }: { connectionId: string; hostUuid: string; rangeSeconds: PerformanceRange; signal?: AbortSignal }): Promise<HostPerformance> {
    return performanceSchema.parse(await readCommand("host_performance", { connectionId, hostUuid, rangeSeconds }, signal))
  },
}
