import { spawn } from "node:child_process"
import { randomBytes } from "node:crypto"
import { fileURLToPath } from "node:url"
const root=fileURLToPath(new URL("../",import.meta.url))
const env={...process.env,XEN_PREVIEW_TOKEN:randomBytes(32).toString("hex")}
const vite=spawn(process.execPath,["node_modules/vite/bin/vite.js"],{cwd:root,env,stdio:["ignore","inherit","inherit"]})
const app=spawn("./target/debug/xen-desktop",process.argv.slice(2),{cwd:root,env,stdio:["pipe","inherit","inherit"]})
process.stdin.pipe(app.stdin)
let stopping=false
async function stop(){
  if(stopping)return;stopping=true
  try{
    const call=async(command,args={})=>{const r=await fetch("http://127.0.0.1:1421/command",{method:"POST",headers:{"Content-Type":"application/json","x-xen-preview-token":env.XEN_PREVIEW_TOKEN},body:JSON.stringify(command==="current_workspace"?{command}:{command,args})});return r.json()}
    const state=await call("current_workspace")
    await Promise.allSettled(state.connections.filter(c=>c.status!=="offline").map(c=>call("disconnect_host",{connectionId:c.id})))
  }catch{}
  vite.kill("SIGTERM");app.kill("SIGTERM");process.exit(0)
}
process.on("SIGINT",stop);process.on("SIGTERM",stop)
app.on("error",error=>{console.error(error.message);stop()})
app.on("exit",stop)
