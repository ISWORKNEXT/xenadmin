# Xen Desktop เทียบ XCP-ng Center และผลทดสอบ dev host จริง

ตรวจวันที่ 11 พฤศจิกายน 2026 ตามเวลา Asia/Bangkok.

รายงานนี้เป็น baseline ก่อนแก้ implementation. ผลและสถานะหลังแก้ติดตามใน [Function parity progress](function-parity-progress.md), รวม New VM wizard, snapshot→VM/template และการแก้ action availability. จำนวนและสถานะในตารางเดิมไม่ใช่ยอดความครบล่าสุด.

Xen Desktop ยังไม่เทียบเท่า XCP-ng Center. หลายคำสั่งพื้นฐานใช้งานได้จริง แต่ยังขาด workflow สำคัญ เช่น VM migration, import/export, New VM wizard, recovery และการตั้งค่าขั้นสูง. การมี XenAPI method หรือปุ่มชื่อเดียวกันไม่ได้ยืนยันว่ามีพฤติกรรมครบต้นฉบับ.

## เวอร์ชันและขอบเขตหลักฐาน

| รายการ | ค่าที่ตรวจ |
|---|---|
| Upstream | [xcp-ng/xenadmin](https://github.com/xcp-ng/xenadmin/tree/ba43130ffb30d05d1eb40187b8ef9f2de111468a) commit `ba43130ffb30d05d1eb40187b8ef9f2de111468a` |
| Local | `development`, commit `bec2480623267555c362ff0b7987ea929ec36050` |
| Local runtime | Linux, Rust 1.99.1, native `egui/eframe` app ที่ build จาก source นี้ |
| Host ที่ทดสอบจริง | `192.168.8.170`, XCP-ng 8.3.0, 1 host, 14 VMs, 5 SRs, 2 networks ก่อนสร้างของทดสอบ |
| Mutation scope | VM, snapshot, VDI, VIF, internal network, file-backed SR, PBD, VMSS และ PVS records ชั่วคราว |
| สิ่งที่ไม่ได้ execute | ไม่ได้รัน WinForms ต้นฉบับบน Windows. ฝั่งต้นฉบับตรวจจาก source ที่ตรึง commit |

ผล unit/integration tests อยู่ใน log เดิมที่รันก่อนผู้ใช้สั่งหยุด mock. หลังคำสั่งนั้น การทดสอบพฤติกรรมใช้ host จริงเท่านั้น. รายงานไม่ใช้ fixture results แทนหลักฐาน live parity.

## ตารางเทียบรายฟังก์ชัน

ตารางหลักมี **299 capability rows**: 44 implemented ในขอบเขตแถว, 108 partial และ 147 missing. อีก 3 แถวระบุ infrastructure. แถวเหล่านี้มีส่วนทับซ้อนกัน จึงไม่ใช่จำนวนฟีเจอร์อิสระหรือเปอร์เซ็นต์ความครบของแอป.

| รายงาน | ขอบเขต | Implemented | Partial | Missing |
|---|---|---:|---:|---:|
| [VM และ storage](audit-vm-storage-20261111.md) | Lifecycle, provisioning, snapshot, migration, import/export, VM properties, VDI/VBD/SR | 19 | 27 | 32 |
| [Host, pool, network และ auth](audit-host-pool-network-20261111.md) | Host administration, pool, HA/WLB/DR/PVS/VMSS, PIF/VIF, AD/RBAC | 19 | 29 | 69 |
| [UI และ monitoring](audit-ui-monitoring-20261111.md) | Tabs, options, performance, alerts, diagnostics, consoles, search, plugins | 6 | 52 | 46 |

สถานะ implemented เป็นการระบุว่าพบ implementation สำหรับพฤติกรรมแคบในแถวนั้น ไม่ได้รับรองทุก version, hardware หรือ end-to-end interaction. ผลกดปุ่มจริงแยกอยู่ใน live UI report.

ไฟล์เสริมระบุ source files รายตัวและแยก wrapper/base class ออกจากฟีเจอร์:

- [Command crosswalk](audit-command-crosswalk-20261111.md): connection commands, folder/tag commands, properties, appliances และ vTPM.
- [Action crosswalk](audit-action-crosswalk-20261111.md): action files ที่เหลือ รวม helpers, DR, updates, custom fields และ status-report source.
- [Wizard และ command controls](audit-wizard-controls-crosswalk-20261111.md): wizard pages และ menu/control wrappers.
- [GUI actions และ plugins](audit-plugin-gui-crosswalk-20261111.md): task adoption, graph/search persistence และ plugin components.
- [Source inventory พร้อมลิงก์และ report references](audit-evidence-20261111/upstream-surfaces.tsv).

Inventory มี 604 non-designer C# files ใน surfaces ที่เลือก: 157 command/control files, 203 model actions, 10 GUI actions, 120 wizard files, 36 settings files, 22 tab files, 34 diagnostics files, 9 options files และ 13 plugin files. Root commands มี 136 ไฟล์. Tab files รวม base classes 2 ไฟล์. จำนวนไฟล์ไม่ใช่จำนวนฟีเจอร์.

Inventory script ตรวจชื่อไฟล์ที่ปรากฏในรายงานเพื่อช่วยนำทาง. การตรวจชื่อไม่ใช่การพิสูจน์ semantic parity และไม่ได้ครอบคลุม generated XenAPI methods ทุก method.

## ช่องว่างที่ผู้ใช้พบในการใช้งานหลัก

| ความต้องการ | สิ่งที่มี | สิ่งที่ยังขาด |
|---|---|---|
| สร้าง VM | Clone template และ provision disks | Wizard เลือก CPU/RAM/SR/network/install media และตรวจความพร้อมครบกระบวนการ |
| ย้าย VM | ย้าย VDI ระหว่าง SR | VM migration ไป host อื่น, cross-pool migration/copy และ destination mapping |
| นำเข้าและส่งออก | ไม่มี workflow | XVA/OVF import/export, appliance และ snapshot export |
| Snapshot | Disk snapshot, revert, delete | Manual checkpoint/quiesce, description/screenshot และ snapshot→VM/template |
| เปลี่ยน resource ขณะ VM ทำงาน | Dynamic memory ในกรณีที่ host อนุญาต | vCPU hotplug, online disk grow, CPU topology/weight และตัวเลือก boot ขั้นสูง |
| สร้าง storage | Host-local file-backed SR | NFS/iSCSI/HBA/FCoE/ISO/CIFS discovery และ creation workflows |
| จัดการ host/pool | คำสั่ง host บางรายการและ pool service settings | Pool create/join/eject, maintenance orchestration, patch/update workflow, certificate install/reset, backup/restore และ host properties หลายส่วน |
| DR/WLB/PVS | มี record/configuration operations บางส่วน | DR recovery/failover/failback/dry-run, WLB recommendation execution และ PVS server-dependent workflows |
| ติดตามระบบ | Fixed RRD charts, basic messages และ health checks | Custom graph persistence, event overlays, typed alerts/remediation/filter/export และ diagnostic equivalence |
| การจัดระเบียบ | VM tags และ named VM searches | Folders, custom fields, tag operations ทุก object, search/grouping breadth และ plugins |

Local และ upstream ต่างมีคำว่า “34 checks” แต่เป็นคนละชุดตรวจ. ห้ามใช้จำนวนที่เท่ากันยืนยันความเทียบเท่า. ดู crosswalk ราย check ในรายงาน UI.

## ผลทดสอบ backend ของแอปกับ host จริง

เรียก public API ของ `xen-core` โดยตรงจาก [parity_live_audit.rs](../crates/xen-core/examples/parity_live_audit.rs). Credentials รับผ่าน stdin และไม่บันทึกลง artifact. ผล mutation ด้านล่างจึงเป็น backend evidence; ไม่ใช่หลักฐานว่ากดปุ่ม UI แล้วทุก flow ผ่าน.

| ชุดทดสอบ | ผลที่สังเกต |
|---|---|
| Inventory, host details, RBAC, pool status, PIF/PBD inventories | อ่านจริงผ่าน |
| Host RRD 15m, 1h, 24h | อ่านจริงผ่าน มี 168 metric series ในแต่ละ range |
| Clone/provision จาก Other install media | สร้าง VM ชั่วคราวได้ |
| Edit name/description, 1 vCPU และ RAM 256 MiB | ผ่าน และอ่าน CPU/memory กลับเพื่อตรวจค่า |
| Tags และ VM HA properties | เขียนผ่าน. Tags มี readback. ไม่ได้ enable pool HA |
| Add VDI/VBD, grow 64→128 MiB | ผ่าน และอ่านขนาดกลับได้ 128 MiB |
| VBD RO/RW, bootable, empty CD drive | คำสั่งสำเร็จ |
| Add/edit/remove halted VIF บน internal network ชั่วคราว | คำสั่งสำเร็จ |
| Halted snapshot, revert, delete | ผ่าน และอ่าน power state หลัง revert เป็น Halted |
| Clone VM และ delete clone | คำสั่งสำเร็จ |
| Start, pause/unpause, force reboot, force shutdown | ผ่าน พร้อม power-state readback |
| Suspend/resume | Suspend ถูก host ปฏิเสธใน allowed-operation guard สำหรับ VM ทดสอบนี้. Resume ไม่ได้รัน จึงไม่ผ่านการ qualification |
| File SR create/scan, PBD unplug/repair, SR detach/reattach | ผ่าน พร้อม attachment readback |
| SR forget/reintroduce/reattach/destroy | คำสั่งสำเร็จและไม่มี SR ชั่วคราวเหลือ |
| File SR probe | ไม่ผ่าน: host ตอบ `INTERNAL_ERROR`. ยังไม่มี successful probe qualification |
| File SR TRIM | ไม่ผ่าน: SR backend นี้ไม่รองรับ TRIM |
| VMSS daily minute=7 | ไม่ผ่าน: `INVALID_VALUE`, ตรวจด้วย `xe` พบ `field: schedule:min`, `value: 7` |
| VMSS daily minute=15 | Create/edit/delete ผ่าน โดย policy ปิดและไม่ผูกกับ VM ใด |
| PVS site/server records | Introduce/forget ผ่าน. ไม่ใช่หลักฐานว่าบริการ PVS proxy/cache ใช้งานได้ |

หลักฐาน:

- [Core live JSONL](audit-evidence-20261111/live-core.jsonl).
- [Storage และ services live JSONL](audit-evidence-20261111/live-services.jsonl).
- [VMSS invalid minute จาก host CLI](audit-evidence-20261111/live-vmss-invalid-minute.log).
- [VMSS valid minute จาก xen-core](audit-evidence-20261111/live-vmss-valid.jsonl).

`PASS` ของแถว “sequence finished”, “Mutation sequence completed” หรือ “Audit result” ใน raw log หมายถึง control flow ถึงจุดจบ. ไม่ได้ลบล้าง `FAIL` ของรายการข้างใน. รายงานนี้ใช้ผลราย check และ readback ตามตาราง ไม่ใช้ sequence label ตัดสินว่าทุกฟีเจอร์ผ่าน.

## ผลเปิดหน้าจอและกดใช้งานจริง

[Live UI report](audit-live-ui-20261111.md) แยกสองชนิดหลักฐาน:

1. เปิด native app จริงด้วย startup flags และตรวจ screenshot จาก dev host. หลักฐานนี้รับรอง route/data rendering เท่านั้น.
2. กดปุ่ม เมนู แท็บ และ dialogs จริง แล้วอ่าน state จาก XenAPI กลับเพื่อตรวจผล. รายการที่ยังไม่ได้กดต้องระบุว่าไม่ได้ทดสอบ.

เปิด startup routes จริง 25 หน้าและตรวจภาพแล้วทั้งหมด. กดและตรวจผลของ `UiAction` 23 variants ในขอบเขตที่บันทึกไว้. อีก 156 variants ยังไม่มีหลักฐานคลิก. ตัวเลขนี้ไม่ใช่ pass rate: `OpenObject` มีทั้งกรณีสำเร็จและ host-tree failure ส่วน `ReloadHostDetails` เป็น failure ที่ทำซ้ำได้.

| Flow ที่กดจริง | ผล |
|---|---|
| Template→New VM→Submit | สำเร็จ มี VM ใหม่จริง |
| Edit→Save | เปลี่ยนชื่อและ vCPU 1→2 สำเร็จ พร้อม readback |
| Add disk→Resize→Remove และ Add VIF→Remove | สำเร็จ. Disk ขยาย 1→2 GiB และตรวจ backend ก่อนลบ |
| Snapshot→Revert→Delete selected | สำเร็จ. ตรวจ record, disk references และ cleanup. ไม่ได้พิสูจน์ guest filesystem contents |
| เปิด Revert confirmation→Cancel | Dialog ปิดโดยไม่มี task ใหม่ |
| Start→Submit | สำเร็จ มี Running และ VBD/VIF attached readback |
| More→Force shut down→Submit | กดแล้วถูกปฏิเสธตาม state guard. ขณะตรวจ VM เป็น Halted แล้ว จึงไม่อ้างว่าปุ่มนี้ทำให้หยุด |
| Delete VM→Submit | สำเร็จ และตรวจไม่มี VM, snapshot, VDI, VBD หรือ VIF ของชุด UI เหลือ |
| Host tree→host→Retry | ล้มเหลวด้วย OpaqueRef/UUID routing bug |
| Hosts card→Host details→CPU และ Performance tabs | สำเร็จ. กดแท็บจริงและแสดงข้อมูล/กราฟได้ |

Screenshots และ action records อยู่ใน [live UI evidence](audit-live-ui-evidence-20261111/). การเห็น framebuffer จาก VNC ไม่ยืนยัน keyboard, mouse, clipboard, SSH login หรือ RDP login.

[Native UI action inventory](audit-evidence-20261111/native-ui-actions.tsv) แจกแจง `UiAction` ทั้ง 179 variants พร้อม declaration, source call sites และ click evidence. ค่า `NOT_VERIFIED_BY_CLICK` หมายถึงยังไม่มีหลักฐานกดจริง. แต่ละ variant อาจมีหลาย object/state/options จึงไม่ใช่จำนวนปุ่ม และการกดหนึ่งกรณีไม่รับรองทุกกรณี. Search fields, theme controls และบาง interaction เปลี่ยน state โดยตรงโดยไม่ผ่าน enum นี้ จึงดูผลใน live UI report เพิ่มเติม.

## ปัญหาที่พบจาก source และ live evidence

| ปัญหา | หลักฐาน | ผลกระทบ |
|---|---|---|
| คลิก host จาก sidebar แล้วเปิด details ไม่ได้ | Live click: [ภาพหน้า error](audit-live-ui-evidence-20261111/clicks/57-host-open-click.png). Source: `sidebar.rs::host_leaf` ใส่ opaque reference ใน `ObjectKey::Host.host_ref`; `app.rs::activate_tab` ส่งค่านี้เป็น `host_uuid` แต่ `Workspace::host_details` ตรวจ host UUID | เส้นทางใช้งานปกติล้มเหลวด้วย “This host is not part of the selected connection.” แม้ startup flag `--host <uuid>` เปิดได้. นี่คือ bug ใน app routing ที่ยืนยันด้วย real UI |
| VMSS minute validation ต่างจาก host | Live: นาที 7 ถูกปฏิเสธ แต่นาที 15 ผ่าน. Source: local `validate_vmss_fields` รับ 0–59; upstream ใช้ quarter-hour values | UI/backend รับค่าที่ server ปฏิเสธ |
| Add interface ใช้รายการ network ที่ไม่ผ่าน visibility filter | Live UI: ค่าเริ่มต้นเป็น Host internal management network. Source: `app.rs::OpenVifDialog` ใช้ทุก inventory network และเลือกตัวแรก; [upstream VIFDialog.LoadNetworks](https://github.com/xcp-ng/xenadmin/blob/ba43130ffb30d05d1eb40187b8ef9f2de111468a/XenAdmin/Dialogs/VIFDialog.cs#L185) ตรวจ `Network.Show`, membership และ SR-IOV eligibility ก่อน | มีโอกาสเลือก internal network โดยไม่ตั้งใจ. รอบทดสอบเลือก Pool-wide network 0 ก่อน submit |
| CHIN ไม่รอ async tunnel task | Source: `network.rs::create_chin_network` อ่าน task ref จาก `Async.Tunnel.create` แล้วทิ้ง | การรับคำขอไม่ยืนยันว่าทุก tunnel สร้างสำเร็จ |
| Session expiry ไม่ทำ reconnect | Source: `workspace.rs::heartbeat_loop` จัด `SESSION_INVALID` ใน generic error branch และ reset failure count | มี heartbeat/reconnect แต่ยังไม่ครอบคลุม session expiry |
| Host actions ขาด upstream orchestration | Source: local เรียก host methods โดยตรง; upstream จัดการ guests, HA tolerance และ coordinator workflows เพิ่ม | ชื่อ action ตรงกัน แต่ผลรวมกระบวนการยัง partial |
| Remove VIF ไม่ทำ unplug อัตโนมัติ | Source: `remove_vif` เรียก `Async.VIF.destroy`; upstream มี unplug-before-destroy workflow | การลบ interface ขณะยัง attached ไม่เทียบเท่า. ยังไม่ได้พิสูจน์ด้วย live attached-VIF deletion ใน audit นี้ |

ยังไม่ได้แก้ application implementation ใน audit นี้. เพิ่มรายงาน, evidence และ audit tools/examples พร้อมใส่ลิงก์ใน gap report เดิมให้ชี้มายังผลตรวจล่าสุด.

## สิ่งที่ยังไม่ qualified

- Host จริงมี host เดียวและ PIF จริงตัวเดียวที่เป็น management interface. ยังไม่ทดสอบ successful cross-host migration, HA/failover, bond หรือ CHIN multi-host.
- ไม่มี successful NFS/iSCSI/HBA creation/probe, SR-IOV hardware setup หรือ WLB/PVS/AD lab qualification ในรอบนี้.
- ไม่ได้ reboot/shutdown host ทั้งเครื่อง หรือทดสอบ boot/recovery กลับมา. การอนุญาตให้ใช้ dev host ไม่ได้ทำให้รายการเหล่านี้ผ่านโดยอัตโนมัติ.
- ไม่ได้ทดสอบ application UI ทุก combination, version, error path หรือ credential-dependent guest service.
- Clean shutdown/reboot, suspend/resume, disk migration/hotplug และหลาย operations มี source implementation แต่ยังไม่มี successful live evidence ใหม่สำหรับทุก operation ในตารางนี้.

## Cleanup และ local checks

Core audit ใช้ชื่อ `zz-parity-root-*` และ services ใช้ `zz-parity-services-*`. อ่าน inventory กลับหลัง cleanup พบ VM/network/SR/VMSS/PVS site ชั่วคราวของสองชุดนี้เหลือ 0. VM เดิมมี UUID, name และ power state ตรงกับ baseline หลัง core audit. นี่ไม่ได้ยืนยันว่า field ทุก field ของระบบไม่เคยเปลี่ยน.

UI audit ใช้ prefix ของตนเองและบันทึก cleanup แยกใน live UI report.

ก่อนหยุด mock รัน workspace tests ได้ 116 passed. `fmt`, Clippy และ native build ผ่านด้วย Rust 1.99.1. `/usr/bin/cargo` เดิมใช้ Rust 1.92.0 ซึ่งเก่าเกิน dependencies. หลังเพิ่ม audit example ตรวจ format และ Clippy อีกครั้งโดยไม่รัน fixtures ซ้ำ.

Logs: [tests](audit-evidence-20261111/tests.txt), [build](audit-evidence-20261111/build.txt), [Clippy หลังเพิ่ม audit tools](audit-evidence-20261111/clippy-after-audit.txt), [format หลังเพิ่ม audit tools](audit-evidence-20261111/fmt-after-audit.txt).

Inventory ที่ตรึง commit สร้างซ้ำด้วย [audit_upstream_inventory.py](../tools/audit_upstream_inventory.py). Script นี้ไม่ติดต่อ Xen host. Live scripts และ Rust example เป็น opt-in audit tools ที่อ่าน credentials ผ่าน stdin. Rust example จำกัด endpoint เป็น dev host ที่ได้รับอนุญาตและมี mutation.
