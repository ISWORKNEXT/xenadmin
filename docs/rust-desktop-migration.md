# แผนย้าย XCP-ng Center เป็น Rust desktop

> Historical note: the current UI is the native `egui` client in `native/`;
> see [`native-egui-port.md`](native-egui-port.md). This document records the
> original Tauri/React assessment and is kept for reference.

วันที่ตรวจ: 9 พฤศจิกายน 2026

เป้าหมายที่ผู้ใช้เลือก: Desktop app ด้วย Tauri, Rust, React และ shadcn/ui

## สถานะ implementation หลังเริ่มย้าย

สร้าง Tauri desktop และ React/shadcn/ui แล้ว Rust รองรับ login, inventory, refresh และ logout
มี UI สำหรับหลาย connection พร้อมกัน, All connections, pool/host scope, VM actions, snapshots, tasks และ per-connection TLS bypass
สถานะการย้ายล่าสุดอยู่ใน `feature-migration-status.md`
ลบ C# projects, solution, resources, branding, installer และ CI ของ .NET ออกจาก working tree ตามคำสั่งผู้ใช้
หลักฐานและ path ของ C# ด้านล่างอ้างถึง commit `8d01db877` ใน Git history
ฟีเจอร์ขั้นสูงในแผนยังไม่ได้ย้าย และยังไม่ประกาศ feature parity

## ขอบเขตการตรวจเดิม

ตรวจรายการไฟล์ทั้ง repository, project references, dependencies, CI และอ่านเส้นทางสำคัญของการเชื่อมต่อ, cache, events, VM start, task polling และ console แล้ว
การตรวจนี้เป็นการสำรวจสถาปัตยกรรม ไม่ใช่การตรวจความถูกต้องทุกบรรทัดของ C# ทั้งหมด
ยังไม่ได้ build โปรแกรม Windows เดิม
หลังสำรวจได้ทดสอบ XenAPI กับ host จริง และเริ่ม Rust transport ตามรายละเอียดด้านล่าง

ฐานโค้ดที่ตรวจคือ commit `8d01db877` และ worktree เดิมไม่มีการแก้ไข
จำนวนต่อไปนี้นับจากไฟล์บนดิสก์ก่อนเพิ่มเอกสาร โดยไม่นับ `.git`
จำนวนบรรทัดรวม comments, generated code และ WinForms Designer

| ส่วน | C# files | C# lines | หน้าที่เดิม |
| --- | ---: | ---: | --- |
| XenAdmin | 1,138 | 247,249 | WinForms, commands, dialogs, wizards, console และ graphs |
| XenModel | 466 | 198,890 | XenAPI client, connection, cache, actions และ search |
| XenOvfApi | 23 | 20,078 | OVF schemas, packaging, validation และ conversions |
| XenCenterLib | 30 | 4,970 | archive, compression และ utilities |
| CommandLib | 5 | 933 | tar, streams และ export utilities |
| root | 1 | ดูไฟล์ CommonAssemblyInfo.cs | assembly metadata |

มี C# รวม 1,663 ไฟล์ และมี .NET projects 5 โครงการ ทุกโครงการ target `net481`
`XenAdmin` เปิด `UseWindowsForms` และ build ActiveX wrapper ของ `mstscax.dll`
CI ปัจจุบัน build Release และ Debug บน Windows แต่ไม่มีขั้นตอนรัน automated tests ใน workflow ที่ตรวจ
ไม่พบ test project ใน solution นี้ แม้จะมี `XenAdmin/TestResources`

## โครงสร้างและพฤติกรรมที่ต้องย้าย

Dependency chain หลักคือ `XenAdmin → XenModel → XenOvfApi → XenCenterLib`
`XenModel` อ้างถึง `CommandLib` เพิ่มอีกทาง

| หลักฐานจากโค้ด | พฤติกรรมที่ต้องคงไว้ใน Rust |
| --- | --- |
| `XenModel/XenAPI/Session.cs`, `JsonRpc.cs`, `JsonRpcClient.cs` | session, JSON-RPC, request IDs, structured errors และ API version compatibility |
| `XenModel/Network/XenConnection.cs` | login, `HOST_IS_SLAVE`, pool coordinator, reconnect, heartbeat และ disconnect |
| `XenModel/XenObjectDownloader.cs` | bootstrap ด้วย `event.from` token ว่าง, event cursor และ add/modify/delete |
| `XenModel/Network/Cache.cs` | object cache และการ resolve ความสัมพันธ์ระหว่าง objects |
| `XenAdmin/Commands/StartVMCommand.cs` | allowed_operations, VM power state, target availability และ broken ISO handling |
| `XenModel/Actions/VM/VMStartAction.cs` | `VM.async_start`, start_on, resume และความคืบหน้าจาก task |
| `XenModel/Actions/AsyncAction.cs` | pending/success/failure/cancelled, error_info, RBAC และ task cleanup |
| `XenModel/XenSearch/MetricUpdater.cs`, `XenAdmin/Controls/CustomDataGraph` | RRD metrics, timestamps, graph history และ server time offset |
| `XenAdmin/ConsoleView/XSVNCScreen.cs`, `XenModel/Network/HTTPHelper.cs` | authenticated HTTP CONNECT สำหรับ VNC |
| `XenAdmin/ConsoleView/RdpClient.cs`, `XenAdmin/XenAdmin.csproj` | Windows ActiveX RDP ซึ่งใช้ใน WebView โดยตรงไม่ได้ |
| `XenModel/Actions/VM/ImportVmAction.cs`, `ExportVmAction.cs`, `XenOvfApi` | streaming, archive, OVF, disk formats, progress และ cleanup เมื่อผิดพลาด |
| `XenAdmin/Network/SSL.cs`, `XenModel/XenAdminConfigManager.cs` | certificates, settings และ proxy configuration |

UI เดิมมีส่วนที่ต้องออกแบบใหม่ครอบคลุม navigation tree, object tabs, bulk selection, dialogs และ multi-step wizards
ชื่อของคำสั่งไม่ใช่ข้อกำหนดทั้งหมด ตัวอย่างเช่น Start VM มีการตรวจ CD drive และ storage ก่อนเริ่มงาน
การแทนด้วยปุ่มที่เรียก `VM.start` อย่างเดียวจึงยังไม่เทียบเท่าโปรแกรมเดิม

## รูปแบบแอปใหม่

React และ TypeScript รับผิดชอบ UI ใน WebView โดยใช้ Vite, Tailwind CSS และ shadcn/ui
Rust รับผิดชอบ XenAPI, sessions, connection lifecycle, cache, operations, transfers และ console transport
Tauri commands เชื่อม UI เข้ากับ Rust และ channels หรือ events ส่งสถานะกลับ UI
shadcn/ui เป็นส่วนของ React UI ดังนั้นแอปนี้ยังมี TypeScript แม้ย้าย backend ออกจาก C# ทั้งหมด

```text
React + shadcn/ui
        |
        | typed commands / subscriptions
        v
Tauri desktop adapter
        |
        v
Rust core: connections, inventory, operations, tasks
        |
        v
XenAPI JSON-RPC / RRD HTTP / VNC CONNECT / transfer streams
        |
        v
XCP-ng / XenServer hosts and pools
```

สำหรับ desktop ที่เลือก ไม่มีความจำเป็นต้องเปิด Rust HTTP API เป็น server ให้ UI
Rust core แยกจาก Tauri เพื่อทดสอบ protocol และ connection lifecycle ได้โดยไม่ต้องเปิดหน้าต่าง
ยังไม่กำหนด framework versions จนกว่าจะเริ่ม implementation และสร้าง lockfiles

โครงสร้างที่เสนออยู่ใน directory ใหม่ โดยใช้ C# เดิมเป็นหลักฐานพฤติกรรมระหว่างย้าย

```text
desktop/
  src/
    components/ui/          # shadcn/ui components
    features/connections/
    features/inventory/
    features/vms/
    features/storage/
    features/networking/
    features/tasks/
    features/console/
    lib/ipc/                # typed Tauri calls
  src-tauri/
    capabilities/
    src/commands/           # desktop boundary
  crates/xen-core/
    src/xapi/               # transport, models, errors
    src/connections/        # sessions, coordinator, reconnect
    src/inventory/          # snapshots, events, cache
    src/operations/         # VM, host, storage, network actions
    src/tasks/              # progress, cancellation, results
    src/metrics/
    src/transfers/
    src/console/
```

เริ่มด้วย core crate เดียวและ desktop adapter ก่อน แยก crates เพิ่มเมื่อมีเหตุผลจาก implementation
ไม่ต้องแปลง WinForms Designer หรือ resource accessors เป็น Rust ทีละไฟล์
ข้อมูลแสดงผลและข้อความที่ยังจำเป็นต้องดึงออกมาใช้ ส่วน layout สร้างใหม่เป็น React

## ขอบเขตระหว่าง UI กับ Rust

- Connection ID เป็น local opaque identifier และแยกจาก XenAPI session reference
- XenAPI object references แยกชนิดจาก UUID และต้องคงค่าที่ host ส่งมา
- Rust เก็บ session reference และ credentials โดยไม่ส่งกลับไปเก็บใน localStorage
- UI ส่ง credentials ให้ Rust ตอน login ได้ แต่ไม่นำ credentials ไปใส่ URL หรือ logs
- แยก connection สำหรับ event long polling เพื่อไม่ให้คำสั่งผู้ใช้ติดรอ event request
- Cache ระบุ connection และ generation เพื่อไม่ใช้ข้อมูลจาก session เก่าหลัง reconnect
- UI รับ snapshot พร้อม revision แล้วตามด้วย changes ที่เรียงลำดับได้
- หาก UI พลาด changes หรือ event cursor หมดอายุ ให้ resync snapshot
- งานที่เปลี่ยน host ส่งกลับ operation handle และติดตามจน terminal state
- หาก timeout หลังส่ง mutation ให้ตรวจผลก่อนตัดสินใจ retry เพื่อไม่ทำงานซ้ำ
- UI ใช้ allowed_operations ช่วยแสดงปุ่ม ส่วน Rust และ host ตรวจสิทธิ์ตอนทำคำสั่งจริง

## ส่วนที่ต้องออกแบบทดแทน

### Console

VNC ต้องมี Rust bridge จาก HTTP CONNECT ไปยัง transport ที่ WebView ใช้ได้
ทางเลือกคือ local WebSocket bridge กับ web VNC client โดยต้องทดลองกับ host จริงก่อนสรุป
หากเปิด local listener ต้อง bind loopback, ตรวจ origin และใช้ short-lived authorization ที่ผูกกับ console หนึ่งชุด
ไม่ส่ง session ของ XenAPI ให้ browser VNC client
ปิด bridge เมื่อเปลี่ยน connection, ปิด console หรือ logout

RDP เดิมอาศัย Windows ActiveX จึงต้องเลือก external RDP client หรือเขียน integration ใหม่
การรองรับ embedded RDP และการเปิด external client เป็นความสามารถต่างกัน ต้องระบุแยกในรายการ parity

### Import และ export

เดิมมี DiscUtils สำหรับ VHD, VHDX, VMDK, WIM และ ISO รวมถึง compression libraries
ยังไม่มีหลักฐานว่า Rust replacement รองรับ formats และ edge cases เท่ากัน
เริ่มจาก native XVA workflow ก่อน แล้วประเมิน OVF และ disk conversion แยกตาม format
ไฟล์ขนาดใหญ่ stream ผ่าน Rust โดยไม่ buffer ทั้งไฟล์ใน React

### Platform และ settings

Tauri ใช้ native WebView ของแต่ละระบบ จึงต้อง build และทดสอบบนแต่ละ OS ที่ต้องการรองรับ
ยังไม่ได้ยืนยันว่า Windows, Linux และ macOS เป็น release targets ทั้งหมด
settings, saved connections, remembered credentials และ certificate trust ต้องมีรูปแบบใหม่
การนำเข้าค่าจากโปรแกรมเก่าต้องเป็นงาน migration ที่ทดสอบแยก โดยเฉพาะข้อมูลเข้ารหัสที่ผูกกับ Windows

## ลำดับการย้ายและเกณฑ์ผ่าน

| ช่วง | ผลลัพธ์ที่ผู้ใช้เห็น | หลักฐานก่อนถือว่าเสร็จ |
| --- | --- | --- |
| 1 | เปิด desktop, Add connection, ดู pool/host/VM/SR/network และ disconnect | build แอปจริง, login host ทดสอบ, coordinator redirect, invalid credentials และ certificate errors |
| 2 | รายการอัปเดตเมื่อ VM เปลี่ยนจาก client อื่น | bootstrap/events ตรงกัน, deletion, reconnect, missed events, stale generation และ logout cleanup |
| 3 | Start, shutdown, reboot, pause, resume และ task progress | ทดสอบ VM disposable, allowed_operations, permission denied, failure, cancel และ read-back power state |
| 4 | สร้าง VM, clone, snapshot และปรับ disk/network | ตรวจ objects และความสัมพันธ์จาก host จริง รวมถึง partial failure cleanup |
| 5 | Console, metrics และ native XVA import/export | keyboard, clipboard ตามขอบเขตที่รองรับ, console reconnect, RRD gaps และไฟล์ขนาดใหญ่ |
| 6 | migration, pool/host operations, updates, HA/DR, advanced hardware และ formats เพิ่มเติม | compatibility matrix, failure scenarios และการเทียบพฤติกรรมกับโปรแกรมเดิมรายฟีเจอร์ |

แต่ละช่วงต้องรักษารายการ supported, partial และ pending แยกกัน
การ build ผ่านหรือมี UI ครบหน้าไม่ใช่หลักฐานว่า feature ทำงานกับ XenAPI จริง
มี host ทดสอบ `192.168.8.170` แล้ว ตรวจ login, read inventory และ logout ผ่าน XenAPI JSON-RPC สำเร็จ
host รายงาน XCP-ng 8.3.0 และชื่อ `OIIO-Server` ยังไม่ได้ทดสอบ mutations

## ข้ามการตรวจ TLS certificate ต่อ connection

Rust transport อยู่ที่ `desktop/crates/xen-core/src/transport.rs`
`TlsOptions.bypass_certificate_verification` มีค่าเริ่มต้นเป็น `false`
เมื่อเปิดเป็น `true` จะข้ามการตรวจ certificate สำหรับ client ของ connection นั้น
ยังคงเชื่อมต่อด้วย HTTPS แต่ certificate ไม่ได้ยืนยันตัวตน server ในโหมดนี้
ไม่มีการปิด TLS verification แบบ global และไม่มี automatic fallback เมื่อ certificate ผิด
client ไม่ตาม HTTP redirects อัตโนมัติ จึงไม่ส่ง session ตาม redirect ไป host อื่น
การเปลี่ยน coordinator ต้องสร้าง connection สำหรับปลายทางใหม่อย่างชัดเจน

UI ที่วางแผนไว้มี checkbox `Bypass TLS certificate verification` ใน Add/Edit connection
ต้องบันทึกค่านี้แยกตาม connection และแสดงสถานะเมื่อเปิดใช้งาน
ขณะนี้เชื่อม Tauri commands และสวิตช์ bypass ใน Add/Edit connection แล้ว

ทดสอบ transport โดยไม่ใช้ credentials ได้ด้วยคำสั่งต่อไปนี้จาก `desktop`

```sh
cargo run -p xen-core --example tls_probe -- https://192.168.8.170
cargo run -p xen-core --example tls_probe -- https://192.168.8.170 --insecure
```

คำสั่งแรกต้องปฏิเสธ certificate ที่ไม่ผ่าน trust ของเครื่อง ส่วนคำสั่งที่สองต้องรับ HTTP response ได้
TLS probe ไม่ได้พิสูจน์ว่า Rust login หรือ inventory ทำงานแล้ว

ผลทดสอบ Rust กับ host จริงวันที่ 9 พฤศจิกายน 2026 คือโหมดปกติปฏิเสธด้วย `UnknownIssuer`
และโหมด `--insecure` ได้ `HTTP 200` โดยทดสอบแยก client ในเวลาเดียวกัน
unit tests 3 รายการผ่าน ครอบคลุม default settings, explicit bypass และการปฏิเสธ origin ที่ไม่ถูกต้อง

## การเปลี่ยนระบบ build

CI ใหม่ต้องตรวจ Rust formatting, lint และ behavior tests ของ RPC, events และ task state
ฝั่ง UI ต้องตรวจ TypeScript และ production build
desktop packaging ต้องรันบน release targets ที่ตกลงแล้ว พร้อมทดสอบ artifact ที่ได้จริง
เก็บ CI ของ C# ไว้ระหว่างที่ยังใช้เทียบพฤติกรรม และเลิกใช้เมื่อ Rust ทดแทนขอบเขต release ได้แล้ว
ต้องคง license และ notices สำหรับโค้ดหรือ assets ที่นำมาใช้ตาม `LICENSE`

## เอกสารอ้างอิงที่ตรวจ

- [Tauri architecture](https://v2.tauri.app/concept/architecture/)
- [Tauri process model](https://v2.tauri.app/concept/process-model/)
- [Tauri project structure](https://v2.tauri.app/start/project-structure/)
- [shadcn/ui with Vite](https://ui.shadcn.com/docs/installation/vite)

ส่วนที่นอกเหนือจาก desktop UI, sessions และ inventory ยังเป็นแผนการย้าย
