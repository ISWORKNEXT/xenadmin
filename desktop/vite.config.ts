import { defineConfig } from "vite"
import react from "@vitejs/plugin-react"
import tailwindcss from "@tailwindcss/vite"
import { fileURLToPath, URL } from "node:url"

export default defineConfig({
  plugins: [react(),tailwindcss(),{
    name:"local-desktop-preview",
    configureServer(server){server.middlewares.use("/api",(req,res,next)=>{
      if(!process.env.XEN_PREVIEW_TOKEN){res.writeHead(503,{"Content-Type":"application/json"});res.end(JSON.stringify({error:"Open the native app or start npm run preview:desktop for a connected browser preview."}));return}
      if(req.method!=="POST" || req.headers["x-xen-desktop-preview"]!=="1" || !["http://127.0.0.1:1420","http://localhost:1420"].includes(String(req.headers.origin))){res.writeHead(403);res.end();return}
      next()
    })},
  }],
  resolve:{alias:{"@":fileURLToPath(new URL("./src",import.meta.url))}},
  server:{host:"127.0.0.1",port:1420,strictPort:true,proxy:{"/api":{target:"http://127.0.0.1:1421",rewrite:path=>path.replace(/^\/api/,""),headers:{"x-xen-preview-token":process.env.XEN_PREVIEW_TOKEN??""}}}},
})
