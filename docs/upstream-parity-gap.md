# Upstream parity gap (non-UI)

วันที่ตรวจ: 10 พฤศจิกายน 2026

## ขอบเขตและ reference

- Upstream: [`xcp-ng/xenadmin`](https://github.com/xcp-ng/xenadmin) commit `ba43130ff` (14 มี.ค. 2026)
- C# baseline ที่ fork ใช้ก่อนลบ: `8d01db877` ซึ่งอยู่เหนือ upstream 2 commits:
  - `59444e5b4` — VM Xen settings, VIF locking, suspend SR, logs
  - `8d01db877` — XenStore keys, VIF addresses, logs, NIC type
- Implementation ที่นำมาเทียบ: `native/` + `crates/xen-core`
- **ไม่นับ UI** — เปรียบเทียบเฉพาะความสามารถของ backend/การทำงานกับ XenAPI และพฤติกรรมของฟีเจอร์

## ขนาดของ upstream เทียบกับของเรา

| ตัวชี้วัด | Upstream | ของเรา |
| --- | ---: | ---: |
| Command classes | 136 | `VmAction` 12 แบบ + connection/task commands |
| Action files | 203 ไฟล์ ใน 31 กลุ่ม | 12 VM actions |
| Wizards | 13 กลุ่ม | 0 |
| Settings panels | 31 | 0 (ฟอร์มแก้ไขมีแค่ connection) |
| Diagnostic checks | 34 | 0 |
| Tab pages | 23 | 7 workspace views + 9 host-detail tabs |
| Friendly error names | 774 | map ไว้ 5 ข้อความ |

## สิ่งที่ทำได้เทียบเท่าหรือใกล้เคียง

| ด้าน | Upstream | ของเรา |
| --- | --- | --- |
| Login/logout | session.login_with_password / logout | เทียบเท่า |
| เลือก pool coordinator | connection manager | ต้องเลือก coordinator เอง; ถ้าต่อ member จะได้ error `HOST_IS_SLAVE` |
| Inventory พื้นฐาน | pool/host/VM/template/snapshot/SR/network | เทียบเท่าข้อมูลหลัก |
| VM power actions | start/shutdown/reboot/force/pause/unpause/suspend/resume | เทียบเท่าคำสั่งหลัก |
| VM edit | name/description/CPU/memory/boot options/ISO/disk/VIF | name/description/boot order ได้ตลอด; CPU/static memory เมื่อ halted; dynamic memory live; ISO/CD; create CD drive; add/resize/remove disk; VIF create/remove/plug | ยังไม่มี disk move, VIF network/locking edit, boot options อื่น |
| Clone | `VM.clone` + set description | clone VM/template ได้, ตั้งชื่อได้, ยังไม่ตั้ง description |
| Template provisioning | New VM from template + provision disks | clone + provision (`VM.provision`) + ล้าง `other_config.disks` |
| Snapshot | create/revert/delete | create/revert/delete + cleanup VBD/VDI ที่เป็นเจ้าของ |
| Destroy VM | destroy + เลือกลบ disks/snapshots | destroy + ลบ owner VBD/VDI, suspend VDI และ snapshots ทั้งหมด (เลือกได้) |
| Task progress/cancel | AsyncAction + task polling | มี progress/cancel/check again |
| Host read-only details | General/NIC/storage/GPU/USB/updates/certs/alerts/performance | มี subset ที่ redact แล้ว |
| Host RRD | host + VM series | host + VM series (RRD updates) |
| Multi-pool | ConnectionsManager | หลาย pool พร้อมกัน, dedupe ด้วย pool UUID |
| Read-only | ใช้ RBAC เป็นหลัก | per-connection + บังคับ production 10.100.1.11 (ส่วนเพิ่มของเรา) |

## สิ่งที่ทำต่างจาก upstream (ไม่ใช่แค่ขาด)

| ด้าน | Upstream | ของเรา | ผลกระทบ |
| --- | --- | --- | --- |
| Inventory sync | `event.from("*")` + cache + token | `event.register`/`event.next` + raw-record cache; rebuild inventory จาก cache และ refresh fallback | ได้ข้อมูลสดจาก client อื่น; ยังไม่เห็น task ของ client อื่น |
| Connection lifecycle | heartbeat ทุก 15 วินาที, reconnect, coordinator failover | heartbeat 15 วินาที, reconnect ด้วย password ใน memory, mark stale หลังพลาด 2 ครั้ง | ไม่มี dialog ขอ credential ใหม่; ถ้า password เปลี่ยนจะ reconnect ไม่ได้ |
| Coordinator | handle `HOST_IS_SLAVE` และสลับไป master | redirect ไป master อัตโนมัติ ระหว่าง login/reconnect | ต้องมี network ไปถึง master |
| VM start | `start_on` / `resume_on`, `assert_can_boot_here`, ตรวจ CD/ISO/SR ที่ broken, HA | `start_on` / `resume_on` + host picker + missing-VDI/SR preflight + `assert_can_boot_here` | ยังไม่ตรวจ HA/ลึกเท่า upstream |
| Snapshot revert | คืน power state และ start/resume บน host เดิม | revert + start/resume อัตโนมัติเมื่อ snapshot ถ่ายตอน Running | ยังไม่เลือก host เดิมแบบเจาะจง |
| Snapshot delete | เลือก snapshots และลบ VBD/VDI ที่เป็นเจ้าของ | ลบ snapshot เดี่ยว หรือลบ snapshots ทั้งหมดพร้อม VM; ยังเลือกบาง snapshot ไม่ได้ | ถ้าต้องการเก็บ snapshot บางรายการต้องลบเองก่อน |
| Clone | ตั้งชื่อ + description | ตั้งชื่อเท่านั้น | description ไม่ตรง |
| Template provisioning | New VM wizard: เลือก CPU/RAM/disk/SR/network, fast clone | clone + provision disks ตาม template, ใช้ค่าเดิมของ template | ยังไม่มี wizard ปรับ CPU/RAM/disk/network ตอนสร้าง |
| Host details | General/NIC/Network/Storage/GPU/USB/Performance/Alerts/HA เต็ม | read-only subset + warnings | ข้อมูลไม่ครบ, แก้ไขไม่ได้ |
| Performance | host + VM RRD, ช่วง 5s/1m/1h/1d, custom graphs, server-time offset | host + VM RRD, 15m/1h/24h, graph คงที่ | จัดกราฟเองไม่ได้, ช่วงเวลาไม่ครบ |
| Alerts | typed alerts + diagnostics checks | host `message` records เท่านั้น | ไม่มี health diagnostics/alert actions |
| Tasks | task ของ host + AsyncAction หลายสถานะ | task ที่เราส่ง + host tasks จาก event stream (read-only); task ของเราเก็บใน memory | เห็น task จาก client อื่นแล้ว แต่ยัง cancel/check งานของ client อื่นไม่ได้ |
| Errors | 774 friendly errors + RBAC errors | 5 mapped messages + generic | ข้อความ error ไม่ละเอียด |
| Search | XenSearch query/group/saved search | substring filter | ค้นหาแบบซับซ้อนไม่ได้ |
| TLS | certificate install/reset/trust | per-connection bypass เท่านั้น | จัดการ certificate ไม่ได้ |
| Proxy | HTTP proxy ตาม settings | ไม่มี | ใช้งานหลัง proxy ไม่ได้ |
| Password saving | encrypted saved passwords + forget | ไม่บันทึกเลย (ตั้งใจ) | ต้องกรอกรหัสทุกครั้ง |
| RBAC | role checks, permissions, warnings | `allowed_operations` + map error | ไม่มี role introspection |
| Tags/folders/custom fields | จัดการได้ | แค่แสดง tags ใน host details | จัดระเบียบทรัพยากรไม่ได้ |

## ฟังก์ชันที่ยังขาด (จัดกลุ่ม)

### 1. VM provisioning และการแก้ไข VM

- New VM wizard, fast create, create from snapshot (create from template มีแล้ว)
- boot options อื่น, disk move ระหว่าง SR
- disk attach จาก VDI ที่มีอยู่, disk move ระหว่าง SR (add/resize/remove มีแล้ว)
- VIF edit network/locking mode (create/delete/plug/unplug มีแล้ว)
- HA restart priority, startup options, other_config
- XenStore data/keys, VM logs
- Tags, folders, custom fields
- vTPM, GPU assignment, USB passthrough, Docker containers
- VM recovery mode, convert to template

### 2. VM lifecycle ที่ยังขาด

- เลือกลบ snapshot บางรายการตอนลบ VM (ตอนนี้ลบทั้งหมดหรือไม่ลบเลย)
- Clone/copy/migrate ภายใน pool และข้าม pool
- Export/import XVA/OVF และ appliance
- Snapshot as template, new VM from snapshot, export snapshot
- Install PV tools, change ISO, CD drive

### 3. Console

- VNC ผ่าน HTTPS CONNECT + framebuffer/keyboard/mouse/clipboard (มีแล้ว)
- RDP (เดิมใช้ Windows ActiveX)
- SSH/text console, RDP; การพิมพ์ Unicode ใช้ X11 keysym (guest ต้องรองรับ); clipboard จำกัด Latin-1 ตาม RFB

### 4. Storage

- SR create/introduce/probe/scan/repair/reattach/trim/destroy
- PBD attach/detach, iSCSI IQN/LUN discovery
- VDI create/destroy/attach/detach/resize/move/migrate
- VBD create/plug/edit
- CBT enable/disable, read caching
- Default SR, SR health, disk cleanup

### 5. Networking

- Create network, bond, VLAN, SR-IOV, CHIN
- Secondary management purpose
- PIF rescan/plug/unplug, PIF properties
- VIF edit network/locking mode (create/delete/plug/unplug มีแล้ว)
- NIC type/address changes (fork feature)

### 6. Host administration

- Enable/disable, maintenance mode, evacuate
- Reboot/shutdown/power-on, restart toolstack
- Change host password, reconnect as, forget password
- License apply/edition
- Control domain memory, autostart, power-on settings
- Multipath edit, iSCSI IQN, logging, AD
- Certificate install/reset, TLS verification
- Apply/upload updates, supplemental packs, CDN sync
- Crash dump download/destroy
- Host backup/restore, status report
- FibreChannel probe, NRPE, plugins

### 7. Pool, HA, DR, WLB, PVS, VMSS

- Create/destroy/join/eject pool
- HA enable/disable/plan
- DR configure/dry-run/failover/failback
- WLB enable/disable/config/recommendations/report
- PVS site/proxy/read caching
- VMSS snapshot policies
- VM appliances start/shutdown/assign
- Pool secret rotation, clustering, SSL legacy, GPU placement

### 8. Monitoring และบริการ

- Typed alerts + AlertSummary actions
- 34 diagnostic checks + health check service
- Performance alerts (Perfmon)
- VM RRD, graph customization, event overlays
- Status reports (client/server), logs

### 9. Authentication และความปลอดภัย

- AD authentication/roles/subjects
- RBAC role/permission UI
- Certificate trust store/install/reset
- Proxy configuration
- Saved password encryption/forget
- Session delegation

### 10. อื่น ๆ

- XenSearch query language, grouping, saved searches
- Plugins, i18n/localization
- Installer/MSI packaging, update check
- XCM conversion manager
- Import settings จาก legacy client

## ลำดับความสำคัญที่แนะนำ

### P0 — ทำให้ใช้งานประจำวันได้

1. **HA precheck** ก่อน start (missing VDI/SR + `assert_can_boot_here` มีแล้ว)
2. **แก้ VM พื้นฐาน**: disk move/attach existing VDI, VIF network/locking edit, boot options อื่น
3. **Bulk snapshot selection** (revert คืน power state แล้ว)
4. **Console RDP + clipboard/SSH console**
5. **Alerts + diagnostics + health check**, host maintenance

### P1 — จัดการโครงสร้างพื้นฐาน

8. Storage/VDI/VBD management
9. Network/VIF/PIF/bond/VLAN/SR-IOV management
10. Host maintenance/reboot/evacuate/power-on
11. Alerts + diagnostics + health check
12. Updates apply/upload + certificate management
13. XVA import/export

### P2 — parity ส่วนที่เหลือ

14. Copy/migrate/cross-pool
15. HA/DR/WLB/PVS/VMSS/VM appliances
16. AD/RBAC
17. XenSearch/tags/folders/custom fields
18. Docker/vTPM/GPU/USB passthrough
19. Status report/backup/plugins/i18n/packaging

## หลักฐานจาก upstream

- `XenAdmin/Commands` 136 commands; `XenModel/Actions` 203 ไฟล์ ใน 31 กลุ่ม
- `XenModel/Network/Heartbeat.cs` — heartbeat ทุก 15,000 ms
- `XenModel/XenObjectDownloader.cs` — `Event.from(session, ["*"], token, timeout)`
- `XenModel/Network/XenConnection.cs` — จัดการ `HOST_IS_SLAVE`, pool master, reconnect, server time offset
- `XenAdmin/Commands/StartVMCommand.cs` — ตรวจ broken CD/SR, target host, allowed operations
- `XenModel/Actions/VM/VMStartAction.cs` — `vm.start_on` / `vm.resume_on`
- `XenModel/Actions/VM/VMSnapshotRevertAction.cs` — คืน power state และ start/resume บน host เดิม
- `XenModel/Actions/VM/VMSnapshotDeleteAction.cs` — ลบ VBD/VDI ที่ snapshot เป็นเจ้าของ
- `XenModel/Actions/VM/VMCloneAction.cs` — clone + set name/description
- `XenModel/XenSearch/MetricUpdater.cs` — RRD query `host=true` (host + VM), interval 5s, server time offset
- `XenAdmin/ConsoleView/XSVNCScreen.cs`, `XenModel/Network/HTTPHelper.cs` — VNC ผ่าน HTTP CONNECT
- `XenModel/XenAPI/FriendlyErrorNames.Designer.cs` — 774 friendly error names
- `XenAdmin/Diagnostics/Checks` — 34 diagnostic checks

## หลักฐานจากฝั่งเรา

- `crates/xen-core/src/operations.rs` — `VmAction` 12 แบบ
- `crates/xen-core/src/xapi.rs` — inventory 6 classes, error mapping 5 ข้อความ
- `crates/xen-core/src/workspace.rs` — task tracking ใน memory, ไม่มี event/heartbeat
- `crates/xen-core/src/performance.rs` — host series only, 3 ช่วง
- `crates/xen-core/src/host.rs` — read-only host sections + redaction
- `crates/xen-core/src/transport.rs` — no proxy, no redirect, per-connection TLS bypass
- `native/src/state.rs` — action gating จาก `allowed_operations` + read-only

## หมายเหตุ

- การทดสอบของเรากับ `192.168.8.170` เป็น read-only และไม่มีการยิง mutation
- ก่อนทำ P0/P1 ควรมี disposable VM/pool สำหรับทดสอบ mutation และทดสอบกับ XCP-ng 8.2/8.3
- ฟีเจอร์ที่ upstream ใช้ Windows-specific (RDP ActiveX, DPAPI, MSI) ต้องออกแบบใหม่ ไม่ใช่ port ตรง ๆ
