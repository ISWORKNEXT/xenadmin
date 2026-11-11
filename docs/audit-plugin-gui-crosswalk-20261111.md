# GUI actions และ plugin implementation crosswalk

Reference audit วันที่ 11 พฤศจิกายน 2026. Upstream commit `ba43130ffb30d05d1eb40187b8ef9f2de111468a` เทียบกับ local commit `bec2480623267555c362ff0b7987ea929ec36050`.

ตารางนี้เสริม inventory ของไฟล์ ไม่ใช่จำนวนฟีเจอร์เพิ่มเติมจาก capability matrices. `Missing` หมายถึงไม่พบ workflow ใน `native/src` และ `crates/xen-core/src`. ส่วนประกอบหลายไฟล์เป็น implementation ของฟีเจอร์เดียวกัน.

## GUI actions

ต้นฉบับอยู่ใต้ [XenAdmin/Actions/GUIActions](https://github.com/xcp-ng/xenadmin/tree/ba43130ffb30d05d1eb40187b8ef9f2de111468a/XenAdmin/Actions/GUIActions).

| ไฟล์ต้นฉบับ | สถานะ | Local และขอบเขตที่ต่าง |
|---|---|---|
| `DismissAlertsAction.cs` | Partial | `operations.rs::dismiss_alert` ลบ message รายตัว. ไม่มี bulk typed-alert dismissal และ alert-specific handling ทั้งชุด |
| `ExternalPluginAction.cs` | Missing | ไม่มี plugin process launcher, target/session argument expansion, cancellation และ stderr/exit-code workflow |
| `GeneralEditPageAction.cs` | Partial | `update_vm` และ `set_vm_tags` แก้ VM ได้. ไม่มี general edit สำหรับทุก object หรือ folder assignment |
| `MeddlingAction.cs` | Partial | `xapi.rs::ExternalTask` และ `views/tasks.rs` แสดงงานภายนอก. ไม่มี external task cancellation และ lifecycle ownership แบบต้นฉบับ |
| `MeddlingActionManager.cs` | Partial | `RawInventory::apply_events` รับ task events แต่ไม่มี manager สำหรับนำงานภายนอกเข้า local action hierarchy |
| `SaveDataSourceStateAction.cs` | Missing | ไม่มี custom graph persistence ผ่าน `pool.set_gui_config` หรือ datasource record/forget workflow |
| `SearchAction.cs` | Partial | `config.rs` บันทึก named VM search ลง local JSON. ไม่มี upstream search definition persistence และ object coverage ทั้งชุด |
| `Wlb/ExportReportAction.cs` | Missing | ไม่มี report rendering และ export report data |
| `Wlb/WlbOptimizePoolAction.cs` | Missing | มี WLB configuration ใน `pool_actions.rs` แต่ไม่มี recommendation execution, VM relocation และ HA orchestration |

## Plugin files

ต้นฉบับอยู่ใต้ [XenAdmin/Plugins](https://github.com/xcp-ng/xenadmin/tree/ba43130ffb30d05d1eb40187b8ef9f2de111468a/XenAdmin/Plugins). Local ไม่มี plugin loader หรือ plugin extension points. ตารางนี้แจกแจงส่วนประกอบของช่องว่างเดียวกัน ไม่ได้นับแต่ละ class เป็นฟีเจอร์อิสระ.

| ไฟล์ต้นฉบับ | สถานะ | หน้าที่ที่ยังไม่มีใน local |
|---|---|---|
| `Placeholders.cs` | Missing | แทน object, address และ delegated-session placeholders ใน plugin arguments และ URI |
| `PluginDescriptor.cs` | Missing | อ่าน plugin XML, metadata, searches, method lists และ enable state |
| `Cmd/PowerShellCmd.cs` | Missing | สร้าง PowerShell process และ function invocation |
| `Cmd/ShellCmd.cs` | Missing | Shell process options, parameter expansion และ required-method declarations |
| `Cmd/XenServerPowerShellCmd.cs` | Missing | XenServer PowerShell invocation พร้อม object/session context |
| `Features/Feature.cs` | Missing | Plugin feature lifecycle, search predicate, metadata และ RBAC method list |
| `Features/MenuItemFeature.cs` | Missing | Plugin menu registration และ shell-command selection |
| `Features/MenuItemFeatureCommand.cs` | Missing | Selection-sensitive plugin command dispatch และ serialization |
| `Features/ParentMenuItemFeature.cs` | Missing | Plugin menu groups และ child features |
| `Features/ParentMenuItemFeatureCommand.cs` | Missing | Selection predicates สำหรับ plugin menu groups |
| `Features/TabPageFeature.cs` | Missing | Plugin HTML tabs, object URL mapping, optional credential/session delegation และ console replacement |
| `UI/TabPageCredentialsDialog.cs` | Missing | Credential prompt และ optional XenAPI Secret persistence สำหรับ plugin tabs |

`PluginManager.cs` อยู่ใน plugin inventory และกล่าวถึงแล้วในรายงาน UI. ไม่มี local plugin manager counterpart.

## Options interface

| ไฟล์ต้นฉบับ | สถานะ | Mapping |
|---|---|---|
| `XenAdmin/Dialogs/OptionsPages/IOptionsPage.cs` | Infrastructure | WinForms options-page interface. Local ใช้ immediate-mode views และ `config.rs`; ไม่ต้อง port interface นี้ตรงตัว. ฟีเจอร์ของ concrete options pages อยู่ในรายงาน UI |

## หลักฐาน local

- [Task UI](../native/src/views/tasks.rs), [event cache](../crates/xen-core/src/xapi.rs), [operations](../crates/xen-core/src/operations.rs), [preferences และ searches](../native/src/config.rs), [pool actions](../crates/xen-core/src/pool_actions.rs).
- การเทียบนี้เป็น source inspection. ผลกดปุ่มจริงและ live mutation อยู่ในรายงานรวมและ live UI report.
