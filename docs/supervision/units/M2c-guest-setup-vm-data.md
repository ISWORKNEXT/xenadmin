# Unit M2c — Guest setup through vm-data (owner extension) and guest tools

Read `docs/supervision/CHARTER.md` first. It is binding.

## Why

Not in upstream. The owner's templates on the dev host (CentOS 6.10 / 7.9 /
Debian 11 "Legacy", AlmaLinux 10 / Debian 12 "Guest Agent") configure the
guest on first boot from `VM.xenstore_data` keys under `vm-data/`. The app
cannot set these today, so a VM made from them in Xen Desktop boots with no
IP, user or password.

## Contract (read these, do not change them)

- `/home/tinnerkun/Desktop/seller/agent/docs/vm-data.md` — key map.
- `/home/tinnerkun/Desktop/game-host/docs/LEGACY-LINUX-TEMPLATES.md` —
  template UUIDs, behaviour, live results.
- `/home/tinnerkun/Desktop/game-host/testing/legacy-template-qualify.py`
  line 142: the field set the legacy guests read:
  `ipv4`, `subnet` (prefix length), `gateway`, `dns1`, `dns2`,
  `login-username`, `ssh-public-keys` (JSON array of strings), `xpwd`.
  Seller agent adds `ip-family`, `submark` (= subnet), `ipv6`,
  `ipv6-gateway`, `ipv6-prefix-length`.
- Templates carry `other_config` markers such as
  `noname-custom-username`, `noname-ssh-public-keys`, `noname-legacy`.
  Use these to decide which fields a template supports.

## Deliver

1. **New VM wizard — "Guest Setup" page** after the Networking page, shown
   only when the chosen template has a `noname-*` marker (or already has
   `vm-data/*` keys). Fields: IP family (IPv4 / dual stack), IPv4 address,
   prefix length, gateway, DNS 1, DNS 2, optional IPv6 set (address,
   prefix, gateway — all three or none), login username (only if
   `noname-custom-username`), SSH public keys (multi-line, one per line;
   only if `noname-ssh-public-keys`), initial password (masked, with
   confirm). Validate formats. On Finish: clone (installed-OS templates do
   **not** call `VM.provision`), write keys with
   `VM.add_to_xenstore_data`/`set_xenstore_data`, read back, and only then
   start the VM if "Start the new VM automatically" is checked. Remove
   stale `vm-data/password`, `vm-data/root-password`, and unused IPv6
   keys as the agent contract says. Do not log the password anywhere.
2. **VM General tab**: a "Guest Setup" section listing the `vm-data/*`
   keys with `xpwd` shown as "•••• (pending guest pickup)". Properties
   dialog gets a "Guest Setup" page to edit them on a halted VM, plus
   "Reset password…" which writes a new `xpwd` (guest applies it on next
   boot, per the legacy doc).
3. **Install Guest Tools…** (VM menu): mount the tools ISO (`guest-tools.iso`
   on the XCP-ng tools SR) into the VM's DVD drive as upstream's
   `InstallToolsCommand` does, with its confirmation text.
4. Never edit or delete the existing templates. Work only on clones named
   `zz-parity-M2c-*`.

## Accept

- fmt, clippy, tests (add unit tests for field validation and the key
  map, including the IPv6 all-or-none rule and stale-key removal).
- Screenshots in `docs/supervision/evidence/M2c/`: wizard Guest Setup
  page, VM General tab Guest Setup section.
- Live on the dev host: create `zz-parity-M2c-debian11` from the Debian
  11 Legacy template through the wizard with a free IP in 192.168.8.x
  (check it does not answer ping first), gateway 192.168.8.1, username
  `paritytester`, one SSH key you generate locally, a random password.
  Start it; within 5 minutes SSH with the key as `paritytester` succeeds
  and `xenstore` shows `vm-data/xpwd` gone from the live guest (or the
  guest reports `setup-status=ok` per the legacy doc). Destroy the VM and
  its disks afterwards. Keep the key and password only in a temp dir you
  delete.
