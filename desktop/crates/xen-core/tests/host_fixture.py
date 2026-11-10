"""Two-host local XenAPI fixture for read-only host details tests.

The fixture binds to loopback HTTPS. It records RPC method names only; request
parameters, credentials, and record contents are never written to the log.
"""
import json
import ssl
import sys
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

pool, cert, key, method_log, failure = sys.argv[1:]
failure_mode, _, failure_method = failure.partition(":")
HOST_A = "host-a-ref"
HOST_B = "host-b-ref"
UUID_A = "host-a-uuid"
UUID_B = "host-b-uuid"


def rec(uuid, name_label, **fields):
    return {"uuid": uuid, "name_label": name_label, **fields}


records = {
    "pool": {"pool-a": rec("pool-a-uuid", "Fixture pool", master=HOST_A, ha_enabled=True)},
    "host": {
        HOST_A: rec(UUID_A, "Fixture host A", address="192.0.2.10", hostname="xen-a",
                    enabled=True, metrics="metrics-a", host_CPUs=["cpu-a"], PIFs=["pif-a"],
                    PBDs=["pbd-a"], PGPUs=["gpu-a"], PUSBs=["usb-a"],
                    software_version={"product_version": "8.3.0", "product_brand": "XCP-ng",
                                      "build_number": "A100", "dbv": "123", "date": "2026-01-02"},
                    certificates=["cert-a"], resident_VMs=["vm-a", "vm-dom0-a"],
                    power_on_config={"password": "CANARY_HOST_POWER_ON"},
                    external_auth_configuration={"secret": "CANARY_EXTERNAL_AUTH", "password": "CANARY_AUTH_CONFIGURATION"},
                    memory_overhead="1073741824", license_params={"key": "CANARY_LICENSE"},
                    other_config={"secret": "CANARY_HOST_OTHER"}),
        HOST_B: rec(UUID_B, "Fixture host B", address="192.0.2.20", hostname="xen-b",
                    enabled=False, metrics="metrics-b", host_CPUs=["cpu-b"], PIFs=["pif-b"],
                    PBDs=["pbd-b"], PGPUs=["gpu-b"], PUSBs=["usb-b"],
                    software_version={"product_version": "9.0.0", "product_brand": "Other"},
                    certificates=["cert-b"], resident_VMs=["vm-b", "vm-dom0-b"]),
    },
    "host_metrics": {
        "metrics-a": {"memory_total": "17179869184", "memory_free": "8589934592",
                       "memory_total_kib": "16777216"},
        "metrics-b": {"memory_total": "34359738368", "memory_free": "1024"},
    },
    "host_cpu": {
        "cpu-a": {"number": 0, "host": HOST_A, "vendor": "FixtureVendorA", "modelname": "Fixture CPU A",
                  "speed": "2400", "flags": "aes avx", "features": "CANARY_CPU_FEATURE_SAFE"},
        "cpu-b": {"number": 0, "host": HOST_B, "vendor": "FixtureVendorB", "modelname": "Fixture CPU B", "speed": "3600"},
    },
    "PIF": {
        "pif-a": rec("pif-a-uuid", "eth0", device="eth0", host=HOST_A, management=True,
                      IP="192.0.2.10", netmask="255.255.255.0", gateway="192.0.2.1",
                      MAC="02:00:00:00:00:10", mtu=1500, VLAN="-1", physical=True,
                      metrics="pif-metrics-a", network="network-a", bond_slave_of="OpaqueRef:NULL",
                      sriov_physical_PIF_of=["sriov-a"]),
        "pif-b": rec("pif-b-uuid", "eth0", device="eth0", host=HOST_B, management=True,
                      IP="192.0.2.20", MAC="02:00:00:00:00:20", physical=True),
    },
    "PIF_metrics": {"pif-metrics-a": {"carrier": True, "speed": "1000000000", "duplex": "full"}},
    "network": {"network-a": rec("network-a-uuid", "Fixture network A", bridge="xenbr0", MTU=1500)},
    "SR": {
        "sr-a": rec("sr-a-uuid", "Fixture SR A", type="ext", physical_size="1000000",
                     physical_utilisation="250000", shared=False, PBDs=["pbd-a"]),
        "sr-b": rec("sr-b-uuid", "Fixture SR B", type="lvm", physical_size="2000000",
                     physical_utilisation="1000", shared=False, PBDs=["pbd-b"]),
    },
    "PBD": {
        "pbd-a": {"host": HOST_A, "SR": "sr-a", "currently_attached": True,
                  "device_config": {"target": "safe-target", "password": "CANARY_PBD_DEVICE_CONFIG"}},
        "pbd-b": {"host": HOST_B, "SR": "sr-b", "currently_attached": True},
    },
    "PGPU": {
        "gpu-a": rec("gpu-a-uuid", "Fixture GPU A", host=HOST_A, PCI="pci-a",
                      supported_VGPU_types=["vgpu-type-a"], enabled_VGPU_types=[]),
        "gpu-b": rec("gpu-b-uuid", "Fixture GPU B", host=HOST_B, PCI="pci-b"),
    },
    "VGPU_type": {
        "vgpu-type-a": rec("vgpu-type-a-uuid", "Fixture vGPU type A", model_name="Fixture vGPU A", framebuffer_size="1073741824"),
        "vgpu-type-b": rec("vgpu-type-b-uuid", "Fixture vGPU type B", model_name="Fixture vGPU B"),
    },
    "PCI": {
        "pci-a": {"pci_id": "0000:01:00.0", "host": HOST_A, "vendor_name": "Fixture GPU Vendor", "device_name": "Fixture GPU"},
        "pci-b": {"pci_id": "0000:02:00.0", "host": HOST_B, "vendor_name": "Other GPU Vendor"},
    },
    "PUSB": {
        "usb-a": rec("usb-a-uuid", "Fixture USB A", host=HOST_A, path="1-2", product_desc="Fixture USB A", vendor_desc="Fixture USB Vendor A", USB_group="OpaqueRef:NULL", attached=False, passthrough_enabled=False),
        "usb-b": rec("usb-b-uuid", "Fixture USB B", host=HOST_B, path="2-1", product_desc="Fixture USB B", vendor_desc="Fixture USB Vendor B", attached=False, passthrough_enabled=False),
    },
    "Certificate": {
        "cert-a": rec("cert-a-uuid", "Fixture host certificate", type="host", fingerprint="AA:BB:CC",
                       not_before="1700000000", not_after="1900000000"),
        "cert-b": rec("cert-b-uuid", "Fixture other certificate", type="host", fingerprint="DD:EE:FF"),
    },
    "message": {
        "alert-host-a": rec("alert-a-uuid", "Host A alert", name="HOST_A_ALERT", cls="host", object="host", obj_uuid=UUID_A,
                             priority=3, body="Alert belongs to host A"),
        "alert-host-b": rec("alert-b-uuid", "Host B alert", name="HOST_B_ALERT", cls="host", object="host", obj_uuid=UUID_B,
                             priority=3, body="Alert belongs to host B"),
        "alert-vm-a": rec("alert-vm-uuid", "VM alert", name="VM_ALERT_CANARY", cls="VM", object="VM", obj_uuid="vm-a-uuid",
                          priority=3, body="Must not appear in host details"),
    },
    "VM": {
        "vm-a": rec("vm-a-uuid", "Fixture VM", power_state="Running", resident_on=HOST_A,
                    is_control_domain=False, is_a_template=False, is_a_snapshot=False, metrics="vm-metrics-a",
                    guest_metrics="guest-metrics-a", memory_dynamic_min="1073741824",
                    memory_static_max="3221225472", memory_overhead="0"),
        "vm-b": rec("vm-b-uuid", "Other VM", power_state="Halted", resident_on=HOST_B,
                    is_control_domain=False, is_a_template=False, is_a_snapshot=False, metrics="vm-metrics-b",
                    guest_metrics="guest-metrics-b", memory_dynamic_min="1073741824",
                    memory_static_max="4294967296"),
        "vm-dom0-a": rec("vm-dom0-a-uuid", "Control domain A", power_state="Running", resident_on=HOST_A,
                         is_control_domain=True, is_a_template=False, is_a_snapshot=False, metrics="vm-metrics-dom0-a",
                         guest_metrics="guest-metrics-dom0-a", memory_overhead="0"),
        "vm-dom0-b": rec("vm-dom0-b-uuid", "Control domain B", power_state="Running", resident_on=HOST_B,
                         is_control_domain=True, is_a_template=False, is_a_snapshot=False, metrics="vm-metrics-dom0-b",
                         guest_metrics="guest-metrics-dom0-b"),
    },
    "VM_metrics": {
        "vm-metrics-a": {"memory_actual": "2147483648", "memory_overhead": "0"},
        "vm-metrics-b": {"memory_actual": "8589934592", "memory_overhead": "268435456"},
        "vm-metrics-dom0-a": {"memory_actual": "1073741824", "memory_overhead": "0"},
        "vm-metrics-dom0-b": {"memory_actual": "1073741824", "memory_overhead": "536870912"},
    },
    "VM_guest_metrics": {
        "guest-metrics-a": {"other": {"feature-balloon": "1"}},
        "guest-metrics-b": {"other": {"feature-balloon": "1"}},
        "guest-metrics-dom0-a": {"other": {}},
        "guest-metrics-dom0-b": {"other": {}},
    },
    "network_sriov": {
        "sriov-a": rec("sriov-a-uuid", "SR-IOV A", host=HOST_A, physical_PIF="pif-a", logical_PIF="pif-a", requires_reboot=False),
        "sriov-b": rec("sriov-b-uuid", "SR-IOV B", host=HOST_B, physical_PIF="pif-b", logical_PIF="pif-b", requires_reboot=False),
    },
    "host_patch": {"patch-a": rec("patch-a-uuid", "Fixture host patch", host=HOST_A, version="1.2.3")},
    "pool_patch": {},
    "pool_update": {"update-a": rec("update-a-uuid", "Fixture update", version="1.2.3", after_apply_guidance="restart_host", hosts=[HOST_A])},
    "host_crashdump": {
        "dump-a": rec("dump-a-uuid", "Fixture crashdump A", host=HOST_A, timestamp="1700000000", size="4096"),
        "dump-b": rec("dump-b-uuid", "Fixture crashdump B", host=HOST_B, timestamp="1700000001", size="8192"),
    },
}

# A true empty list must agree with the selected host's advertised references.
if failure_mode == "empty" and failure_method == "PUSB.get_all_records":
    records["host"][HOST_A]["PUSBs"] = []

lock = threading.Lock()


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *_args):
        pass

    def do_POST(self):
        request = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        method = request.get("method", "")
        with lock, open(method_log, "a", encoding="utf-8") as log:
            log.write(method + "\n")

        result = None
        error = None
        params = request.get("params", [])
        if method == "session.login_with_password":
            result = "fixture-session"
        elif method == "session.logout":
            result = None
        elif failure_mode == "denied" and method == failure_method:
            error = {"message": "RBAC_PERMISSION_DENIED"}
        elif failure_mode == "unknown" and method == failure_method:
            error = {"message": "MESSAGE_METHOD_UNKNOWN"}
        elif failure_mode == "empty" and method == failure_method:
            result = {}
        elif failure_mode == "missing" and method == failure_method:
            result = {}
        elif method.endswith(".get_all_records"):
            klass = method[:-len(".get_all_records")]
            if klass in records:
                result = records[klass]
            else:
                error = {"message": "MESSAGE_METHOD_UNKNOWN"}
        elif method.endswith(".get_by_uuid"):
            klass = method[:-len(".get_by_uuid")]
            uuid = params[-1] if params else ""
            result = next((ref for ref, value in records.get(klass, {}).items() if value.get("uuid") == uuid), "OpaqueRef:NULL")
        elif method.endswith(".get_record"):
            klass = method[:-len(".get_record")]
            ref = params[-1] if params else ""
            result = records.get(klass, {}).get(ref, {})
        elif method == "host.get_servertime":
            result = "2026-11-09T00:00:00Z"
        elif method == "network_sriov.get_remaining_capacity":
            result = 7
        else:
            error = {"message": "MESSAGE_METHOD_UNKNOWN"}

        body = json.dumps({"jsonrpc": "2.0", "id": request["id"], "result": result, "error": error}).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)


server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
context.load_cert_chain(cert, key)
server.socket = context.wrap_socket(server.socket, server_side=True)
print(server.server_port, flush=True)
server.serve_forever()
