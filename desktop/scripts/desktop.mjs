import { spawn } from "node:child_process"
import { existsSync } from "node:fs"
import { homedir } from "node:os"
import { delimiter, join } from "node:path"

const env = { ...process.env }
env.PATH = [join(homedir(), ".cargo", "bin"), env.PATH].filter(Boolean).join(delimiter)
// Optional user-local development headers on hosts without system devel packages.
const sysroot = join(homedir(), ".cache", "xen-tauri-build", "sysroot")
if (process.platform === "linux" && existsSync(sysroot)) {
  env.PKG_CONFIG_PATH = [join(sysroot, "usr/lib64/pkgconfig"), join(sysroot, "usr/share/pkgconfig"), env.PKG_CONFIG_PATH].filter(Boolean).join(delimiter)
  env.PKG_CONFIG_SYSROOT_DIR = sysroot
  env.LIBRARY_PATH = [join(sysroot, "usr/lib64"), env.LIBRARY_PATH].filter(Boolean).join(delimiter)
}
const executable = process.platform === "win32" ? "tauri.cmd" : "tauri"
const child = spawn(executable, process.argv.slice(2), { env, stdio: "inherit" })
child.on("error", error => { console.error(error.message); process.exitCode = 1 })
child.on("exit", code => { process.exitCode = code ?? 1 })
