#!/usr/bin/env python3
"""Read-only GUI evidence against the real authorized dev host. No fixtures.

Password is requested on an echo-disabled terminal and passed to the app via
stdin. It is never persisted, placed in argv/environment, or printed.
Requires Python Xlib/Pillow and an existing X11/Xwayland session.
"""
import argparse
import getpass
import json
import os
from pathlib import Path
import ssl
import subprocess
import tempfile
import time
import urllib.request
from PIL import Image
from Xlib import display


def windows(root):
    for child in root.query_tree().children:
        yield child
        yield from windows(child)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--host', default='192.168.8.170', choices=['192.168.8.170'])
    parser.add_argument('--user', default='root')
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--settle-seconds', type=float, default=7)
    parser.add_argument('--case', action='append')
    args = parser.parse_args()
    password = getpass.getpass('Authorized dev-host password (hidden): ')
    endpoint = 'https://' + args.host + '/jsonrpc'
    ctx = ssl._create_unverified_context()
    req_id = 0

    def rpc(method, params):
        nonlocal req_id
        req_id += 1
        request = urllib.request.Request(endpoint, data=json.dumps({
            'jsonrpc': '2.0', 'id': req_id, 'method': method,
            'params': params}).encode(), headers={'Content-Type': 'application/json'})
        with urllib.request.urlopen(request, context=ctx, timeout=45) as response:
            value = json.load(response)
        if value.get('error'):
            # Deliberately do not print server error data or raw responses.
            raise RuntimeError('XenAPI operation failed: ' + method)
        return value.get('result')

    session = rpc('session.login_with_password', [args.user, password, '1.0', 'Xen Desktop audit read-only'])
    try:
        hosts = rpc('host.get_all_records', [session])
        vms = rpc('VM.get_all_records', [session])
        srs = rpc('SR.get_all_records', [session])
        networks = rpc('network.get_all_records', [session])
        host = next(iter(hosts.values()))['uuid']
        candidates = [v for v in vms.values() if not v.get('is_control_domain')
                      and not v.get('is_a_template') and not v.get('is_a_snapshot')
                      and not v.get('name_label', '').startswith('rental-order-')]
        candidates.sort(key=lambda v: (not v.get('name_label', '').startswith('zz-parity-root-'),
                                       v.get('power_state') != 'Running', v.get('name_label', '')))
        vm = candidates[0]['uuid'] if candidates else None
        user_srs = [v for v in srs.values() if v.get('content_type') != 'iso'
                    and v.get('type') not in ('udev',) and v.get('name_label') != 'XenServer Tools']
        sr = (user_srs or list(srs.values()))[0]['uuid']
        network = next(v['uuid'] for v in networks.values() if v.get('bridge') != 'xenapi')
    finally:
        rpc('session.logout', [session])
        session = None
    repo = Path(__file__).resolve().parents[1]
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    dpy = display.Display()
    results = []
    startup = {'settings': {'host': args.host, 'username': args.user,
                            'label': 'AUTHORIZED DEV HOST / READ ONLY', 'read_only': True,
                            'tls': {'bypass_certificate_verification': True}}, 'password': password}
    cases = [('overview', []), ('vms', ['--view', 'vms']),
             ('templates', ['--view', 'templates']), ('tasks', ['--view', 'tasks']),
             ('health', ['--health']), ('auth', ['--auth']),
             ('pool-status', ['--pool-status']), ('storage', ['--sr', sr]),
             ('network', ['--network', network])]
    cases += [('host-' + tab, ['--host', host, '--host-tab', tab]) for tab in
              ['general', 'cpu', 'performance', 'networking', 'storage', 'hardware',
               'updates', 'certificates', 'health']]
    if vm:
        cases += [('vm-' + tab, ['--vm', vm, '--vm-tab', tab]) for tab in
                  ['general', 'performance', 'storage', 'network', 'ha', 'snapshots', 'console']]
    print(json.dumps({'live_inventory': {'host_count': len(hosts), 'guest_candidates': len(candidates),
                      'sr_count': len(srs), 'network_count': len(networks)},
                      'selected_vm': vm, 'read_only': True}), flush=True)
    with tempfile.TemporaryDirectory(prefix='xen-live-gui-') as temporary:
        for name, flags in cases:
            if args.case and name not in args.case:
                continue
            env = os.environ.copy()
            env.pop('WAYLAND_DISPLAY', None)
            env.update(XDG_CONFIG_HOME=str(Path(temporary) / name), WINIT_UNIX_BACKEND='x11')
            with (output / (name + '.log')).open('w') as log:
                app = subprocess.Popen([str(repo / 'target/debug/xen-native'), '--connect-stdin', *flags],
                                       env=env, stdin=subprocess.PIPE, stdout=log, stderr=log, text=True)
                try:
                    app.stdin.write(json.dumps(startup) + '\n')
                    app.stdin.close()
                    time.sleep(args.settle_seconds)
                    target = None
                    pid_atom = dpy.intern_atom('_NET_WM_PID')
                    for window in windows(dpy.screen().root):
                        try:
                            pid = window.get_full_property(pid_atom, 0)
                            if pid is not None and int(pid.value[0]) == app.pid:
                                geo = window.get_geometry()
                                if geo.width > 100 and geo.height > 100:
                                    target = window
                                    break
                        except Exception:
                            pass
                    captured = False
                    if target is not None:
                        geo = target.get_geometry()
                        raw = target.get_image(0, 0, geo.width, geo.height, 2, 0xffffffff)
                        Image.frombytes('RGB', (geo.width, geo.height), raw.data,
                                        'raw', 'BGRX').save(output / (name + '.png'))
                        captured = True
                    result = {'route': name, 'alive_after_settle': app.poll() is None,
                              'window_captured': captured, 'settle_seconds': args.settle_seconds,
                              'exit_code_before_cleanup': app.poll(),
                              'visual_review': 'pending'}
                    results.append(result)
                    (output / 'live-results.json').write_text(json.dumps(results, indent=2) + '\n')
                    print(json.dumps(result), flush=True)
                finally:
                    if app.poll() is None:
                        app.terminate()
                    try:
                        app.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        app.kill()
                        app.wait()
    password = None
    startup = None
    if not all(v['alive_after_settle'] and v['window_captured'] for v in results):
        raise SystemExit(1)


if __name__ == '__main__':
    main()
