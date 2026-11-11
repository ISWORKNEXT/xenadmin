# การทดสอบ Native UI กับ dev host จริง — 11 พฤศจิกายน 2026

ทดสอบ `target/debug/xen-native` กับ `192.168.8.170` (XCP-ng8.3.0,OIIO-Server) บน X11/Xwayland จริง ไม่มี mock/fixture ถูกใช้ในช่วงงานนี้ ไม่แก้ application source และไม่แตะ production

แบ่งหลักฐานเป็นสองส่วน:25 startup/deep-link routes เป็นการตรวจภาพที่แอปแสดงจาก host จริง; การคลิก XTEST จริงเป็นอีกชุดหนึ่ง พร้อม API readback แบบอ่านอย่างเดียว การเปิดหน้าสำเร็จไม่เท่ากับทดสอบทุกปุ่ม

## ผลสำคัญและข้อบกพร่อง

1. **เปิด Host จาก Infrastructure tree ไม่ได้**:คลิก Hosts→OIIO-Server แล้วหน้าแสดง OpaqueRef แทนUUID และ error “This host is not part of the selected connection.” กด Retry ได้ error เดิม ([57](audit-live-ui-evidence-20261111/clicks/57-host-open-click.png),[58](audit-live-ui-evidence-20261111/clicks/58-host-retry-click.png)) `native/src/views/sidebar.rs::host_leaf` ใส่ `host.reference` ใน `ObjectKey::Host.host_ref`; `native/src/app.rs::activate_tab` ส่งค่านี้เป็น `Command::HostDetails.host_uuid` ขณะที่ `workspace.rs::host_details` ตรวจ hostUUID จึงไม่ผ่าน การเปิดจาก Hosts card→Host details ใช้UUIDและทำงานได้ ([62](audit-live-ui-evidence-20261111/clicks/62-host-card-open.png)) จึงเป็น bug ของเส้นทางใช้งานปกติที่ startup flag เพียงอย่างเดียวตรวจไม่พบ
2. **Add interface เลือก Host internal management network เป็นค่าเริ่มต้น** แม้หน้า Networks ปกติกรอง networkนี้ออก ([32](audit-live-ui-evidence-20261111/clicks/32-add-vif-dialog.png),[33](audit-live-ui-evidence-20261111/clicks/33-vif-network-options.png)) ครั้งนี้เลือก Pool-wide network0 เองก่อน submit นี่คือช่องว่าง UX/ค่าเริ่มต้นที่ควรทบทวน ไม่ได้อ้างว่าการ attach internal network ทำให้ระบบเสียหายเพราะไม่ได้ทดลองค่านั้น
3. **Force shut down ยังไม่ผ่าน**:กด menu/confirmation/submit จริง แต่ backend ปฏิเสธว่า hostไม่อนุญาต operationใน stateปัจจุบัน ([47](audit-live-ui-evidence-20261111/clicks/47-force-stop-confirm.png),[48](audit-live-ui-evidence-20261111/clicks/48-force-stop-result.png)) APIตอนตรวจพบ Haltedแล้ว แต่ไม่มี force-shutdown task จึงห้ามนับเป็นผลสำเร็จของปุ่ม Guestเปล่าอาจหยุดเองก่อน submit;สาเหตุการหยุดเองไม่ได้พิสูจน์ในงานนี้ การ startก่อนหน้านั้นยืนยัน Running จริง

## การคลิก mutation จริงที่ทำและตรวจกลับ

ใช้เฉพาะ VM/VDI/snapshot prefix `zz-parity-ui-*`;ไม่ได้ส่ง lifecycle/inputให้ VMของผู้ใช้ `rental-order-*` หรือ Windows/XOAเดิม API helperใช้เฉพาะ login/get_all_records/logoutเพื่อตรวจผล;mutationทุกขั้นด้านล่างทำจากปุ่มของแอป

| ขั้น | การคลิก/กรอกจริง | ผลและหลักฐาน |
|---|---|---|
| ค้น template | คลิกช่องsearch,กรอก `name:Other`,คลิกชื่อ Other install media |51→1 templateและเปิดรายละเอียด ([11](audit-live-ui-evidence-20261111/clicks/11-other-template.png),[13](audit-live-ui-evidence-20261111/clicks/13-template-open.png)) |
| Create from template | New VM from template→กรอก `zz-parity-ui-20261111`→Submit |task succeeded;APIพบ VMใหม่Halted,1vCPU,256MiB,not-template ([16](audit-live-ui-evidence-20261111/clicks/16-clone-result.png)) |
| แก้ VM | tree search→เปิด VMตัวเอง→Edit/Resources→แก้ชื่อ/description/vCPU→Save |ชื่อ `zz-parity-ui-20261111-edited`,2/2vCPU,256MiB;APIยืนยัน ([23](audit-live-ui-evidence-20261111/clicks/23-edit-saved.png)) RAMคง256MiB ไม่ได้ทดสอบเปลี่ยน RAM |
| Add disk | Disks→Add disk→Local storage→1GiB→ชื่อ `zz-parity-ui-disk`→Add |VDI/VBDใหม่,แสดง1GiB ([27](audit-live-ui-evidence-20261111/clicks/27-disk-added.png)) GUIรับจำนวนเต็มGiB จึงใช้1GiBแทน64MiB |
| Resize disk | Manage→Resize→2GiB→Resize |UIแจ้งสำเร็จและ backend `virtual_size=2147483648` ([30](audit-live-ui-evidence-20261111/clicks/30-disk-resized.png),[backend](audit-live-ui-evidence-20261111/clicks/backend-readback.json)) |
| Add VIF | Network→Add interface→เปลี่ยนnetworkเป็นPool-wide network0→Add |device0/MACใหม่,backendอ่านกลับตรงกัน ([34](audit-live-ui-evidence-20261111/clicks/34-vif-added.png)) |
| Snapshot | Take snapshot→ชื่อ `zz-parity-ui-snapshot`→Submit |task succeeded;APIพบ snapshotและsnapshotVDI ([37](audit-live-ui-evidence-20261111/clicks/37-snapshot-result.png)) |
| Cancel confirmation | Snapshots→Revert→Cancel |dialogปิด ไม่มี taskใหม่;จากนั้นเปิด Revertอีกครั้ง ([40](audit-live-ui-evidence-20261111/clicks/40-revert-cancelled.png)) |
| Revert | Revert→Submit |task succeeded,VMยังHalted;backendพบ parentVDI/VBDชุดใหม่ ([41](audit-live-ui-evidence-20261111/clicks/41-revert-result.png)) ไม่ได้เขียนไฟล์ในguestเพื่อพิสูจน์ข้อมูลfilesystemย้อนกลับ |
| Delete snapshot | เลือกsnapshotตัวเอง→Delete selected→Delete snapshots |APIไม่พบsnapshotและsnapshotVDI ([42](audit-live-ui-evidence-20261111/clicks/42-delete-snapshot-confirm.png),[43](audit-live-ui-evidence-20261111/clicks/43-snapshot-deleted.png)) |
| Start | Start→Submit |task succeeded,API Running,VBD/VIF attached=true ([45](audit-live-ui-evidence-20261111/clicks/45-start-result.png)) ไม่ใช่ผลติดตั้งOSหรือguest tools |
| Force shut down | More→Force shut down→Submit |**CLICKED_REJECTED** ตามข้อ3 ไม่ใช่PASS |
| Remove VIF | Network→Remove→Remove interface |API vifs=[] ([50](audit-live-ui-evidence-20261111/clicks/50-vif-removed.png)) |
| Remove owned disk | Disks→Manage→Remove→คงDelete from storage checked→Remove disk |API vbds=[]และvdis=[] ([52](audit-live-ui-evidence-20261111/clicks/52-disk-removed.png)) |
| Delete VM | More→Delete→ตรวจชื่อ→Submit |task succeeded;APIไม่พบVM/VDI/VBD/VIF prefixนี้ ([55](audit-live-ui-evidence-20261111/clicks/55-vm-deleted.png),[final-cleanup](audit-live-ui-evidence-20261111/clicks/backend-readback.json)) |
| Host navigation | คลิกHosts leaf→Retry;จากนั้นHosts list→Host details card |leafล้มเหลว,cardทำงานตามข้อ1 |
| Host tabs | คลิกCPU & memoryและPerformanceจริงจากcardroute |ข้อมูลCPU/memoryและ168metricsแสดง ([63](audit-live-ui-evidence-20261111/clicks/63-host-cpu-click.png),[64](audit-live-ui-evidence-20261111/clicks/64-host-performance-click.png)) |

## การเปิด25หน้าด้วย startup flags

ทุกหน้าด้านล่างใช้ read_only=true profileและconfigชั่วคราว รอ7วินาทีแล้วอ่านภาพหน้าต่างจริง ผลเต็มใน [live-results.json](audit-live-ui-evidence-20261111/live-results.json) รูปอยู่ directoryเดียวกัน แต่ไม่ได้ scrollตรวจทุกแถวหรือคลิกทุกcontrol

| หน้า | สิ่งที่เห็นจริง |
|---|---|
| [overview](audit-live-ui-evidence-20261111/overview.png) | Connected pool summary rendered (startup connection selects pool; not the global Home dashboard). |
| [vms](audit-live-ui-evidence-20261111/vms.png) | Real guest list rendered,14 visible guests at capture; read-only actions disabled. |
| [templates](audit-live-ui-evidence-20261111/templates.png) | 51 real templates rendered. |
| [tasks](audit-live-ui-evidence-20261111/tasks.png) | Owned tasks empty for this fresh process; external XenAPI tasks displayed. |
| [health](audit-live-ui-evidence-20261111/health.png) | 34 cached health checks rendered:0 critical,2 warning,10 info,22 OK at capture. |
| [auth](audit-live-ui-evidence-20261111/auth.png) | Local authentication,1 protected root subject,6 roles rendered; write buttons disabled. |
| [pool-status](audit-live-ui-evidence-20261111/pool-status.png) | HA/WLB disabled; PVS/VMSS/DR zero at capture; fields rendered. |
| [storage](audit-live-ui-evidence-20261111/storage.png) | Local ext SR,attached PBD,capacity and19 user VDI rows rendered. |
| [network](audit-live-ui-evidence-20261111/network.png) | Pool-wide network0,bridge xenbr0,management eth0/PIF and VIF counts rendered. |
| [host-general](audit-live-ui-evidence-20261111/host-general.png) | Real identity/software/uptime rendered; maintenance mode explicitly Unavailable. |
| [host-cpu](audit-live-ui-evidence-20261111/host-cpu.png) | 4 CPU records,63.89GiB physical memory and allocation groups rendered. |
| [host-performance](audit-live-ui-evidence-20261111/host-performance.png) | 168 real RRD metrics,four CPU series,15-minute chart,actual60-second samples rendered. |
| [host-networking](audit-live-ui-evidence-20261111/host-networking.png) | One eth0 management interface and hostIP rendered. |
| [host-storage](audit-live-ui-evidence-20261111/host-storage.png) | Four host storage attachment rows rendered,including local ext/ISO/udev. |
| [host-hardware](audit-live-ui-evidence-20261111/host-hardware.png) | One Intel GPU and seven PCI devices rendered in initial viewport; lower USB section not fully reviewed. |
| [host-updates](audit-live-ui-evidence-20261111/host-updates.png) | Empty updates/patches section rendered; not proof of update apply capability. |
| [host-certificates](audit-live-ui-evidence-20261111/host-certificates.png) | Two host/host_internal public certificate metadata records rendered. |
| [host-health](audit-live-ui-evidence-20261111/host-health.png) | Empty alerts and crashdump sections rendered. |
| [vm-general](audit-live-ui-evidence-20261111/vm-general.png) | Running Windows10 guest details,4vCPU/24GiB fields rendered read-only. |
| [vm-performance](audit-live-ui-evidence-20261111/vm-performance.png) | 37 real VM RRD metrics/four CPU series rendered. |
| [vm-storage](audit-live-ui-evidence-20261111/vm-storage.png) | Existing ISO+64GiB disk and CD media displayed read-only. |
| [vm-network](audit-live-ui-evidence-20261111/vm-network.png) | Existing eth0/MAC and attached status rendered read-only. |
| [vm-ha](audit-live-ui-evidence-20261111/vm-ha.png) | Existing VM HA priority/affinity/delay/resource fields rendered read-only. |
| [vm-snapshots](audit-live-ui-evidence-20261111/vm-snapshots.png) | One pre-existing guest snapshot displayed read-only; no action on it. |
| [vm-console](audit-live-ui-evidence-20261111/vm-console.png) | Actual1920x1200 Windows desktop framebuffer displayed via VNC; no guest input sent. |

## ขอบเขตการยืนยัน

- [ui-action-coverage.json](audit-live-ui-evidence-20261111/clicks/ui-action-coverage.json) ระบุเฉพาะ23 UiAction variantsที่มีการคลิกจริงพร้อมข้อจำกัด `OpenObject`มีทั้งpassและhost-leaf fail; `ReloadHostDetails`เป็น CLICK_VERIFIED_FAILURE; Propose/SubmitIntentไม่ได้หมายถึงทุก ActionKindผ่าน Controlsที่ไม่มีในไฟล์นี้ยังไม่ได้ยืนยันด้วยการคลิก
- [actions.json](audit-live-ui-evidence-20261111/clicks/actions.json) เป็นลำดับจริงของdriverรอบสุดท้าย ไม่ใส่password/session;รูป00–09เป็นการแก้automation focus/Thai-keyboardก่อนเริ่มmutation ไม่ใช่bugแอปและไม่ใช่PASSของsearch ชุดที่ใช้ได้เริ่มรูป10
- ไม่ได้ทดสอบทุกmenu/setting/dialog, saved-search save,CD insert/eject,hotplug,move disk,clean shutdown/reboot,pause/suspend/resume,taskcancel,reconnect,HA/WLB/PVS/VMSS/DR,AD,host destructive actions,SSH/RDP login,graphช่วงอื่นหรือclipboard/keyboard/mouseส่งเข้าguest บางส่วนมีหลักฐานbackendแยกในรายงานหลัก แต่ยังไม่ใช่UIclick coverage
- VNCของguestเดิมยืนยันเฉพาะlive framebuffer ไม่ส่งkey,mouse,Ctrl+Alt+Delหรือclipboard Guest SSH/RDP credentialsไม่ได้มีในงานนี้
- จำนวนVMในรูปต่างช่วงเปลี่ยนได้เพราะrootทดสอบtemporary resourcesพร้อมกัน นี่ไม่ใช่inventory regressionจากการดูภาพต่างเวลา
- Cleanupอ่านกลับprefixทั้งหมดเป็นศูนย์ทั้งVM/snapshot,VDI,VBD,VIF รวมoriginalVDI `dc120783-2653-40a7-85c6-28c03501ae8d`,snapshotVDI `f15cd468-e8ce-481e-86aa-6bab8c344d3b`,revertedVDI `6179684d-3c11-4231-9c3e-4214fe43d8f7` ไม่มีทรัพยากรUIที่สร้างไว้เหลืออยู่ rootมีSSHreadbackoriginal UUIDเพิ่มใน `docs/audit-evidence-20261111/live-ui-original-objects-cleanup.txt`
- รหัสผ่านรับทางecho-disabled stdinและส่งให้appทางstdinเท่านั้น ไม่ใส่argv/env/fileหรือภาพ ไม่เก็บsession token/configส่วนตัว ภาพอาจมีชื่อVM/UUIDของlabซึ่งเป็นหลักฐานข้อมูลจริงของผู้ใช้
- Utilityสุดท้ายใช้temporary spare-key mappingสำหรับพิมพ์ASCIIในThai keyboard layoutและคืนmappingทุกครั้ง ไม่มีhardcoded keyboard-group restoreเหลืออยู่ในscript;รอบทดลองก่อนหน้าคืนค่าgroup1ที่ตรวจพบเดิมแล้ว
