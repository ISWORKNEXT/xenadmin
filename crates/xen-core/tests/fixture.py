"""Local HTTPS XenAPI fixture. No request leaves loopback."""
import json, ssl, sys, threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
pool, cert, key = sys.argv[1:]
state = {"power": "Halted", "starts": 0, "polls": 0, "cancelled": False}
lock = threading.Lock()
class Handler(BaseHTTPRequestHandler):
    def log_message(self, *args): pass
    def do_POST(self):
        request = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        method, params = request["method"], request["params"]
        error = None
        with lock:
            if method == "session.login_with_password": result = "fixture-session"
            elif method == "session.logout": result = None
            elif method == "pool.get_all_records": result = {"pool-ref": {"uuid": pool, "name_label": pool}}
            elif method == "host.get_all_records": result = {"host-ref": {"uuid": pool+"-host", "name_label":pool, "address":"127.0.0.1", "enabled":True, "metrics":"metrics-ref", "host_CPUs":[], "software_version":{}}}
            elif method == "host_metrics.get_all_records": result = {"metrics-ref":{"memory_total":"4096","memory_free":"2048"}}
            elif method == "VM.get_all_records": result = {"vm-ref": {"uuid":"same-vm-uuid", "name_label":"Same VM name", "power_state":state["power"], "allowed_operations":["start","clean_shutdown"],"is_control_domain":False,"is_a_template":False,"is_a_snapshot":False,"resident_on":"host-ref"}}
            elif method in ["SR.get_all_records","network.get_all_records"]: result = {}
            elif method == "VM.get_by_uuid": result = "vm-ref"
            elif method == "VM.get_record": result = {"allowed_operations":["start"],"is_control_domain":False,"is_a_template":False,"is_a_snapshot":False}
            elif method == "Async.VM.start":
                assert params[-2:] == [False, False]
                state["starts"]+=1; result="task-ref"
            elif method == "task.get_record":
                state["polls"]+=1
                status="pending"
                if state["cancelled"]: status="cancelled"
                elif state["polls"]>1 and pool!="cancel-pool": status="failure" if pool=="failure-pool" else "success"
                if status=="success": state["power"]="Running"
                result={"status":status,"progress":1 if status=="success" else .2,"allowed_operations":["cancel"] if status=="pending" else [],"error_info":["NOT_ENOUGH_FREE_MEMORY"] if status=="failure" else []}
            elif method == "task.cancel": state["cancelled"]=True; result=None
            elif method == "task.destroy": result=None
            else: result=None; error={"message":"MESSAGE_METHOD_UNKNOWN"}
        body=json.dumps({"jsonrpc":"2.0","id":request["id"],"result":result,"error":error}).encode()
        self.send_response(200);self.send_header("Content-Type","application/json");self.send_header("Content-Length",str(len(body)));self.end_headers();self.wfile.write(body)
server=ThreadingHTTPServer(("127.0.0.1",0),Handler)
context=ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER);context.load_cert_chain(cert,key)
server.socket=context.wrap_socket(server.socket,server_side=True)
print(server.server_port,flush=True)
server.serve_forever()
