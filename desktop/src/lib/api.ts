import { invoke, isTauri } from "@tauri-apps/api/core"
import { z } from "zod"

const settingsSchema = z.object({ host:z.string(),username:z.string(),label:z.string().default(""),read_only:z.boolean().default(false),tls:z.object({bypass_certificate_verification:z.boolean()}) })
const hostSchema=z.object({reference:z.string(),uuid:z.string(),name:z.string(),address:z.string(),version:z.string(),brand:z.string(),enabled:z.boolean(),cpu_count:z.number(),memory_total:z.number(),memory_free:z.number()})
const vmSchema=z.object({reference:z.string(),uuid:z.string(),name:z.string(),description:z.string(),power_state:z.string(),vcpus:z.number(),memory:z.number(),os:z.string(),kind:z.string(),allowed_operations:z.array(z.string()),host_name:z.string(),resident_on:z.string(),snapshot_of:z.string(),snapshot_time:z.string(),disk_count:z.number(),interface_count:z.number()})
const storageSchema=z.object({uuid:z.string(),name:z.string(),description:z.string(),storage_type:z.string(),total:z.number(),used:z.number(),shared:z.boolean()})
const networkSchema=z.object({uuid:z.string(),name:z.string(),description:z.string(),bridge:z.string(),mtu:z.number(),interface_count:z.number(),virtual_interface_count:z.number()})
const inventorySchema=z.object({settings:settingsSchema,pool_id:z.string(),pool_name:z.string(),hosts:z.array(hostSchema),vms:z.array(vmSchema),templates:z.array(vmSchema),snapshots:z.array(vmSchema),storage:z.array(storageSchema),networks:z.array(networkSchema)})
const connectionBase=z.object({id:z.string(),settings:settingsSchema})
const connectionSchema=z.discriminatedUnion("status",[
  connectionBase.extend({status:z.literal("offline")}),
  connectionBase.extend({status:z.literal("online"),inventory:inventorySchema}),
  connectionBase.extend({status:z.literal("stale"),inventory:inventorySchema,error:z.string()}),
])
const operationSchema=z.object({id:z.string(),connection_id:z.string(),connection_name:z.string(),vm_uuid:z.string(),object_name:z.string(),action:z.string(),status:z.enum(["running","succeeded","failed","cancelled","unreachable"]),progress:z.number(),message:z.string(),can_cancel:z.boolean(),started_at:z.number()})
const workspaceSchema=z.object({connections:z.array(connectionSchema),tasks:z.array(operationSchema)})
export type Workspace=z.infer<typeof workspaceSchema>
export type ConnectionView=z.infer<typeof connectionSchema>
export type ConnectionSettings=z.infer<typeof settingsSchema>
export type VirtualMachine=z.infer<typeof vmSchema>
export type Inventory=z.infer<typeof inventorySchema>
export type Operation=z.infer<typeof operationSchema>
export type ActionKind="start"|"shutdown"|"reboot"|"force_shutdown"|"force_reboot"|"pause"|"unpause"|"suspend"|"resume"|"revert"
export type VmAction={kind:ActionKind}|{kind:"clone"|"snapshot";name:string}

async function command(name:string,args:Record<string,unknown>={}):Promise<unknown> {
  if(isTauri()) return invoke<unknown>(name,args)
  const response=await fetch("/api/command",{method:"POST",headers:{"Content-Type":"application/json","X-Xen-Desktop-Preview":"1"},body:JSON.stringify(name==="current_workspace"?{command:name}:{command:name,args})})
  if(!response.headers.get("content-type")?.includes("application/json")) throw new Error("The desktop preview backend is not running. Open the native app or start npm run preview:desktop.")
  const result:unknown=await response.json()
  if(!response.ok) {const parsed=z.object({error:z.string()}).safeParse(result);throw new Error(parsed.success?parsed.data.error:"The desktop backend could not complete this request.")}
  return result
}
export const api={
  async current():Promise<Workspace>{return workspaceSchema.parse(await command("current_workspace"))},
  async connect(settings:ConnectionSettings,password:string,profileId:string|null):Promise<string>{return z.string().parse(await command("connect_host",{settings,password,profileId}))},
  async refresh(connectionId:string){await command("refresh_connection",{connectionId})},
  async disconnect(connectionId:string){await command("disconnect_host",{connectionId})},
  async remove(connectionId:string){await command("remove_connection",{connectionId})},
  async action(connectionId:string,vmUuid:string,action:VmAction):Promise<Operation>{return operationSchema.parse(await command("start_vm_action",{connectionId,vmUuid,action}))},
  async check(taskId:string){await command("check_task",{taskId})},
  async cancel(taskId:string){await command("cancel_task",{taskId})},
}
export function errorMessage(error:unknown):string {if(error instanceof Error)return error.message;if(typeof error==="string")return error;return "Something went wrong. Please try again."}
export function connectionName(connection:ConnectionView):string {return connection.settings.label.trim() || (connection.status!=="offline"?connection.inventory.pool_name:connection.settings.host)}
export function bytes(value:number):string {if(!value)return "0 B";const units=["B","KiB","MiB","GiB","TiB"];const index=Math.min(Math.floor(Math.log(value)/Math.log(1024)),units.length-1);return `${(value/1024**index).toLocaleString(undefined,{maximumFractionDigits:1})} ${units[index]}`}
