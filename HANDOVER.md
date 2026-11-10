# Xen Desktop — เอกสารส่งมอบงาน

อัปเดต: 11 พฤศจิกายน 2026
เวอร์ชันซอฟต์แวร์: 0.1.0 (tag ล่าสุด `v0.1.4`)
สาขาหลัก: `development`

> เอกสารนี้สรุปสิ่งที่สร้างเสร็จ วิธีใช้งาน วิธี build/ปล่อย release ผลการทดสอบ
> และรายการที่ยังต้องใช้ lab/สิทธิ์จากเจ้าของระบบเพื่อทดสอบต่อ

---

## 1. สรุปผู้บริหาร

Xen Desktop คือไคลเอนต์เดสก์ท็อป native สำหรับจัดการ XenServer/XCP-ng
เขียนใหม่ด้วย **Rust + egui/eframe** แทน XCP-ng Center เดิม (WinForms)
โดยคงโครงสร้างการใช้งานแบบ Infrastructure tree + object tabs + contextual toolbar
แต่ใช้หน้าตาแบบใหม่ รองรับธีมสว่าง/มืด และขยาย UI 110%

สถานะปัจจุบัน:

- ฟังก์ชันหลัก P0/P1 ถูก implement แล้วเกือบทั้งหมด
- **116 unit/integration tests ผ่าน**
- CI ผ่าน (`fmt`, tests, `clippy -D warnings`, build)
- GitHub Actions build และปล่อย **Linux x86_64 + Windows x86_64** อัตโนมัติเมื่อ push tag `v*`
- งานที่เหลือส่วนใหญ่เป็น **live verification ที่ต้องมี lab** (multi-host, PVS, DR,
  SR-IOV/CHIN, AD, guest credentials, destructive host actions)

---

## 2. ลิงก์สำคัญ

| รายการ | ลิงก์ |
|---|---|
| Repository | https://github.com/ISWORKNEXT/xenadmin |
| Releases | https://github.com/ISWORKNEXT/xenadmin/releases |
| CI workflow | `.github/workflows/checks.yml` |
| Release workflow | `.github/workflows/release.yml` |
| คู่มือ native UI | `docs/native-egui-port.md` |
| ช่องว่างเทียบ upstream | `docs/upstream-parity-gap.md` |
| Host information parity | `docs/host-information-parity.md` |

---

## 3. สถาปัตยกรรมและโครงสร้างโปรเจกต์

Cargo workspace มี 2 crate:

- `crates/xen-core` — ตรรกะ XenAPI, cache, operations, models (ไม่ผูกกับ UI)
- `native` — แอปเดสก์ท็อป egui/eframe

### 3.1 `crates/xen-core/src`

| ไฟล์ | หน้าที่ |
|---|---|
| `xapi.rs` | JSON-RPC transport, login, session, inventory records, event cache |
| `transport.rs` | HTTP/TLS/proxy transport |
| `workspace.rs` | หลาย connection, lifecycle lock, inventory cache, wrappers |
| `operations.rs` | VM/VBD/VIF/VDI/SR/network operations และ task handling |
| `vm.rs` | VM details, disks, interfaces, snapshots, VDI catalog |
| `host.rs` | Host details และ performance-related records |
| `performance.rs` | RRD parsing/models |
| `network.rs` | PIF catalog, bond/VLAN/SR-IOV/CHIN create |
| `storage.rs` | PBD catalog, SR probe/repair |
| `auth.rs` | Authentication/RBAC, AD enable/disable, subject roles |
| `pool_status.rs` | HA/WLB/PVS/VMSS/DR/SR-IOV read models |
| `pool_actions.rs` | HA enable/disable, WLB configure/enable/disable/deconfigure |
| `vmss.rs` | VM snapshot schedule create/update/delete |
| `pvs.rs` | PVS site/server/proxy/cache-storage actions |
| `dr.rs` | DR task create/destroy |
| `health.rs` | 34 read-only diagnostics checks |
| `console.rs` | VNC console session plumbing |

### 3.2 `native/src`

| ไฟล์ | หน้าที่ |
|---|---|
| `main.rs` | eframe entry point, CLI flags |
| `app.rs` | Application state machine, actions/events |
| `worker.rs` | Background Tokio worker, commands/events |
| `state.rs` | UI state, dialogs, tabs, load states |
| `config.rs` | Profiles/UI preferences/saved searches (XDG/APPDATA/macOS) |
| `theme.rs`, `widgets.rs` | Design system |
| `console.rs` | VNC framebuffer/input/clipboard |
| `ssh.rs` | Embedded SSH text console + terminal buffer |
| `rdp.rs` | Embedded IronRDP console |
| `views/` | หน้า UI ทั้งหมด (sidebar, inventory, pool, VM, host, dialogs, ฯลฯ) |

---

## 4. ฟีเจอร์ที่ส่งมอบ

สถานะความหมาย:

- **[Live]** ทดสอบกับ XenAPI จริงบน dev host แล้ว
- **[Impl]** implement + unit tests แล้ว แต่ live test ต้องมี lab
- **[Guard]** ทดสอบเฉพาะ validation/guard path กับ host จริง

### 4.1 Connection / Inventory / UI

- หลาย pool พร้อม session แยก, หน้า Overview รวมทุก connection
- Infrastructure tree + object tabs + contextual toolbar + status bar
- XenAPI event stream แทน polling, heartbeat 15s และ reconnect
- TLS bypass ต่อ connection, HTTP/SOCKS proxy ต่อ connection
- ธีมสว่าง (ค่าเริ่มต้น)/มืด, UI scale 110%, ซูมด้วย `Ctrl+Plus/Minus/0`
- Production connection บังคับ read-only ที่ชั้น Rust backend

### 4.2 VM lifecycle และ snapshots

- Start / start on host / resume / resume on host (มี preflight missing VDI/SR)
- Clean shutdown / reboot / force shutdown / force reboot
- Pause / unpause / suspend / resume
- Clone, delete VM (owned disks + snapshot cascade แบบเลือกได้)
- Take snapshot, revert (คืน power state), delete snapshot, bulk snapshot delete
- สร้าง VM จาก template (clone + provision + refresh inventory)

### 4.3 VM editing / resources / HA / tags

- ชื่อ, description, tags
- vCPUs max/at-startup, static memory, dynamic memory min/max
- **Slider** สำหรับ vCPU และ memory ในกล่อง Edit / Resources
- BIOS/UEFI, secure boot, boot order, boot policy, after-crash
- HA restart priority, start/shutdown delay, affinity host
- การเปลี่ยน vCPU/static memory ทำได้เมื่อ VM shut down; dynamic memory ปรับได้ขณะรัน

### 4.4 Disks / storage

- Add disk, attach existing VDI, resize, remove (เลือก delete VDI ได้)
- RO/RW mode (halted), bootable flag, hot-plug/unplug ขณะรัน
- Move disk ระหว่าง SR:
  - VM halted → copy VDI + swap VBD
  - VM running → `VDI.pool_migrate` (live) **[Live]**
- SR: create (file-backed), introduce/forget, probe, repair, reattach/detach,
  scan, TRIM, destroy **[Live สำหรับ file-backed]**
- PBD plug/unplug, user-VDI inventory + delete (unattached/writable เท่านั้น)

### 4.5 Networking (host/pool)

- Internal network, bond, VLAN
- PIF plug/unplug พร้อม management protection
- SR-IOV logical network create **[Guard]**
- CHIN (GRE/VXLAN tunnel) create **[Guard]**
- Destroy network ทำความสะอาด VLAN, bond, network_sriov, tunnel ก่อน
- Read-only SR-IOV/tunnel/internal-network discovery **[Live]**

### 4.6 VM networking

- เพิ่ม/ลบ/plug/unplug VIF
- แก้ network และ locking mode
- แสดง MAC, IPv4/IPv6, attached state
- เข้าถึงได้จากแท็บ Network, ปุ่ม Network บนหัว VM และ Quick actions

### 4.7 Consoles

- Embedded VNC ผ่าน HTTPS CONNECT tunnel
  - framebuffer, mouse, wheel, keyboard/Unicode, Ctrl+Alt+Del
  - clipboard two-way (ตามข้อจำกัด RFB)
- Embedded SSH text console (credential prompt, ANSI terminal, Ctrl+C)
- Embedded RDP ด้วย IronRDP (credential prompt, framebuffer, mouse/keyboard/wheel,
  Ctrl+Alt+Del) + external `xfreerdp` fallback
- Quick action **VNC console** เชื่อมต่ออัตโนมัติเมื่อ VM รัน

### 4.8 Pool services

- HA enable/disable (shared heartbeat SR + failure tolerance) **[Guard]**
- WLB configure/enable/disable/deconfigure **[Guard]**
- VMSS snapshot schedule create/edit/delete **[Live lifecycle]**
- PVS site/server create+remove **[Live]**, proxy/cache actions **[Impl]**
- DR task create/destroy **[Guard]**
- Pool status page รวม HA/WLB/PVS/VMSS/DR/SR-IOV

### 4.9 Diagnostics / search / auth

- Health & diagnostics 34 read-only checks **[Live]**
- XenSearch: fields/comparators, AND/OR/NOT, parentheses, saved searches, grouping
- Authentication & RBAC read-only view
- AD enable/disable **[Guard]**
- Role assignment สำหรับ directory subjects (local root protected)

### 4.10 UX ล่าสุด

- ปุ่มบนหัว VM: **Console**, **Disks**, **Network**, Refresh
- การ์ด **Quick actions** ในหน้า General:
  VNC console / Disks / Network / Snapshots / Edit / Resources…
- VNC auto-connect เมื่อเปิด Console ของ VM ที่รันอยู่
- Slider ใน Edit / Resources และ Add/Resize disk
- กล่อง Edit เลื่อนได้ กรอบ Save/Cancel ไม่หลุดจอ
- ปุ่ม Start/Resume ไม่แสดงสี primary เมื่อกดไม่ได้

---

## 5. วิธี build / run

### 5.1 สิ่งที่ต้องมี

- Rust stable (แนะนำ 1.99+; ต้องมี `rustfmt`, `clippy`)
- Linux: pkg-config, libxkbcommon, libwayland, libx11, libxcb
- Windows: Visual Studio Build Tools (MSVC) + Rust target `x86_64-pc-windows-msvc`

```bash
source "$HOME/.cargo/env"
```

### 5.2 Typecheck / lint / test / build

```bash
cargo check --workspace --all-targets --locked
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --release -p xen-native --locked
```

ผลลัพธ์ไบนารี:

- Linux: `target/release/xen-native`
- Windows: `target/release/xen-native.exe`

### 5.3 Run

```bash
./target/release/xen-native
```

Flags ที่มี:

```
--connect-stdin   รับ JSON {settings,password} ทาง stdin
--view            overview | vms | hosts | templates | storage | networks | tasks
--vm UUID         เปิด VM/template/snapshot
--vm-tab          general | performance | storage | network | ha | snapshots | console
--health          เปิด Health & diagnostics
--auth            เปิด Authentication & RBAC
--pool-status     เปิด Pool status
--sr UUID         เปิด Storage repository
--network UUID    เปิด Network
--search QUERY    เปิด XenSearch
--group-by        none | connection | host | power | tag
--saved-search NAME
--theme           light | dark
--on-top
```

### 5.4 CI / Release

- `.github/workflows/checks.yml`
  - ทุก push/PR: fmt → tests → clippy → build
- `.github/workflows/release.yml`
  - ทุก tag `v*`: build matrix
    - `ubuntu-22.04` → `xen-desktop-<tag>-linux-x86_64.tar.gz`
    - `windows-latest` → `xen-desktop-<tag>-windows-x86_64.zip`
  - สร้าง `SHA256SUMS`
  - publish GitHub Release อัตโนมัติ

การปล่อยเวอร์ชันใหม่:

```bash
git tag -a v0.1.5 -m "Xen Desktop 0.1.5"
git push git@github.com:ISWORKNEXT/xenadmin.git v0.1.5
# GitHub Actions จะ build + publish เอง
```

---

## 6. ผลการทดสอบ

### 6.1 Unit/integration tests

```
cargo test --workspace --locked
- xen-core:  89 passed
- host:       4 passed
- workspace:  5 passed
- native:    18 passed
รวม 116 passed, 0 failed
```

### 6.2 CI

- `Rust desktop checks` ผ่านบน `ubuntu-24.04`
- `Release` ผ่าน: Linux + Windows builds และ publish release

### 6.3 Live verification บน dev host

ทดสอบกับ XenAPI จริง (192.168.8.170) แล้ว:

- Inventory/event stream, proxy, heartbeat
- VM lifecycle, snapshot revert/delete, bulk snapshot
- VM edit (name/tags/HA), VBD RO/RW + hot-plug, disk move offline และ live
- SR create/introduce/forget/repair/reattach/destroy (file-backed)
- PBD plug/unplug, PIF plug/unplug + management guards
- Bond/VLAN/SR-IOV/CHIN **guard paths**
- PVS site/server lifecycle
- VMSS create/edit/delete (ไม่มี VM ถูกแก้)
- Health diagnostics 34 checks
- Auth/RBAC read-only + AD validation guards
- Pool HA/WLB/DR validation guards โดยไม่เปลี่ยน state
- ไม่เหลือ `zz-*` VM/SR/network หลัง cleanup

### 6.4 Live verification ที่ยังค้าง

| รายการ | เหตุผล |
|---|---|
| HA/WLB apply จริง | ต้องมี multi-host pool + shared heartbeat SR + WLB server |
| PVS proxy/cache จริง | ต้องมี PVS server |
| DR task create/destroy จริง | ต้องมี shared NFS/iSCSI/HBA ที่ probe ได้ |
| SR-IOV/CHIN สำเร็จ | ต้องมี NIC/switch รองรับ + multi-host |
| SR probe สำเร็จ | file backend ตอบ `INTERNAL_ERROR`; ต้องมี NFS/iSCSI/LVM |
| Embedded RDP/SSH login | ต้องมี guest credentials |
| AD enable/disable จริง | ต้องมี AD/LDAP lab |
| Host reboot/shutdown/evacuate | ต้องได้รับอนุญาตบน host จริง |
| TRIM สำเร็จ | SR ที่มีไม่รองรับ trim |

---

## 7. ความปลอดภัยและการปฏิบัติการ

- **ห้าม commit credential** — ใน repo ไม่มีรหัสผ่าน
- รหัสผ่านถูกเก็บในหน่วยความจำเฉพาะ session ของผู้ใช้
- Password fields ถูก mask และไม่ถูกบันทึกใน config
- External auth config ถูก redact ค่า `password/secret/credential/keytab`
- Production `10.100.1.11` ถูกบังคับ read-only ที่ backend
- Dev host `192.168.8.170` ใช้ทดสอบ; สร้างเฉพาะ VM ชื่อ `zz-*` และลบหลังทดสอบ
- ห้ามแตะ VM `rental-order-*` ของผู้ใช้
- ก่อน release: `cargo clippy -D warnings`, tests, release build และ smoke run

---

## 8. ประวัติ release

| Tag | รายละเอียด |
|---|---|
| `v0.1.0` | Linux build ด้วยมือ (asset เดียว) |
| `v0.1.1` | GitHub Actions build Linux + Windows ครั้งแรก |
| `v0.1.2` | แก้โครง zip ให้แบน + ตั้งชื่อ release สวยงาม |
| `v0.1.3` | VM quick actions, VNC auto-connect, sliders, scrollable edit |
| `v0.1.4` | รวม UX polish (ปุ่ม Start แสดง disabled ถูกต้อง) — tag `73b12e078`; Actions build Linux/Windows |

> ตรวจสถานะ asset ล่าสุดได้ที่หน้า Releases ด้านบน

---

## 9. งานที่ต้องทำต่อ (ต้องใช้ lab/สิทธิ์)

1. **Multi-host pool + shared SR** — ทดสอบ HA enable/disable, failover, evacuate,
   live migration ข้าม host, host reboot/shutdown
2. **WLB server** — configure/enable/disable/deconfigure จริง
3. **PVS server** — proxy create/destroy และ cache storage จริง
4. **Shared NFS/iSCSI/HBA** — DR task create/destroy, SR probe/repair/trim สำเร็จ
5. **SR-IOV NIC/switch + multi-host** — SR-IOV/CHIN create จริง
6. **AD/LDAP lab** — AD enable/disable และ subject role assignment จริง
7. **Guest credentials (SSH/RDP)** — ทดสอบ embedded consoles จริง
8. **Destructive host permission** — reboot/shutdown/evacuate บน host จริง

งานที่ไม่ต้องรอ lab แต่ยังทำได้:

- ปรับปรุง UX เพิ่มเติม (เช่น keyboard shortcuts, bulk operations)
- เพิ่ม integration tests สำหรับ worker commands
- ปรับ installer/code signing (ถ้าต้องการแจกจ่ายวงกว้าง)
- ปรับ auto-update/version check

---

## 10. เอกสารอ้างอิงใน repo

- `README.md` — ภาพรวม ฟีเจอร์ วิธีรัน และ flags
- `docs/native-egui-port.md` — สถานะ port และตารางฟีเจอร์
- `docs/upstream-parity-gap.md` — ช่องว่างเทียบ XCP-ng Center
- `docs/host-information-parity.md` — รายละเอียด host information
- `CHANGELOG.md` — ประวัติการเปลี่ยนแปลง
- `.github/workflows/` — CI และ release pipeline

---

## 11. วิธีทดลองใช้งานเร็ว

1. เปิดแอป → **Add connection** → กรอก host/user/password
2. เลือก pool ใน Infrastructure tree
3. เปิด VM → กด **Console** (มุมขวาบน) หรือ **VNC console** ใน Quick actions
4. ปรับ CPU/RAM: **Edit / Resources…** → ลาก slider (ต้อง shut down สำหรับ vCPU/static RAM)
5. จัดการดิสก์: **Disks** → Add / Resize (slider) / Move / Plug / Remove
6. จัดการเน็ตเวิร์ก: **Network** → Add interface / Edit / Plug / Remove
7. ดูสถานะ pool: **Pool status** → HA/WLB/PVS/VMSS/DR/SR-IOV
8. ตรวจสุขภาพ: **Health check** → 34 checks
9. จัดการสิทธิ์: **Authentication** → subjects/roles/AD

---

## 12. ผู้รับผิดชอบ / การส่งต่อ

- Source of truth: branch `development`
- Release artifacts: GitHub Releases (build โดย Actions)
- หากต้องทดสอบส่วนที่ค้าง: กรุณาจัดเตรียม lab ตามหัวข้อ 9 และแจ้ง credentials
  ผ่านช่องทางที่ปลอดภัย (ห้ามใส่ใน git)
- เมื่อ lab พร้อม สามารถเปิด issue/แจ้งเพื่อทำ live verification และปิด P1 ที่เหลือได้ทันที
