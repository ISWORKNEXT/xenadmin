# Supervision charter

Goal: Xen Desktop has the same UX and functions as XCP-ng Center
(upstream `xcp-ng/xenadmin` at `ba43130ff`, local copy at
`/home/tinnerkun/Desktop/xen/upstream-xenadmin`). The supervisor (Claude)
owns scope, order, UX decisions, review, commits to `development`, and pushes.
Workers (codex, opencode) execute one unit brief at a time.

## Fixed decisions

1. **Layout follows upstream.** Menu bar, toolbar, navigation pane, and
   per-object tab sets match XCP-ng Center in structure, naming and order.
   The visual skin (colors, fonts, spacing tokens in `native/src/theme.rs`)
   stays ours. When in doubt, open the upstream source and copy its
   information architecture, labels and enable/disable rules.
2. **Excluded:** permissions/RBAC/AD subjects and certificate work. Their
   menu items are shown disabled.
3. **No-lab features** (migration, pool join/eject, HA failover, NFS/iSCSI/HBA,
   GPU/USB passthrough): port from upstream source, test what one host allows,
   label the issue `untested-lab`.
4. **Release** only at milestone end. Workers never tag, push, or touch CI.
5. **Tracking is GitHub issues via `gh`** (label `parity`). The supervisor
   comments results and closes issues; no status spreadsheets or scripts.

## Worker rules

- Do exactly the unit brief. Anything else you notice goes in the
  "Found, not fixed" section of your report. Do not fix it.
- Do not write design docs, qualification docs, or new audit tools.
  The only docs you edit are the ones the brief names.
- Verification budget per unit, and no more:
  1. `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
     `cargo test --workspace` (use `~/.cargo/bin/cargo`, Rust 1.99.1).
  2. For a host-facing function: one real happy-path run on `192.168.8.170`
     with a XenAPI readback, plus one rejection case if upstream guards it.
  3. For UI: launch the real app, click the path once, save 1–3 screenshots
     under `docs/supervision/evidence/<unit>/`.
  4. Remove only objects you created (`zz-parity-<unit>-*`).
  Do not test concurrency, disconnect recovery, huge inputs, or other
  scenarios unless upstream implements that behavior.
- Host credentials come from env `XEN_DEV_HOST`, `XEN_DEV_USER`,
  `XEN_DEV_PASSWORD`. Never write them to a file, log, or commit.
- The dev host `192.168.8.170` is a full-freedom lab (owner's decision,
  2026-11-12). When the unit's function needs it you may reboot the host,
  restart the toolstack, enter/exit maintenance mode, create and destroy
  SRs and networks, and start/stop/modify pre-existing VMs. Two working
  rules keep the lab usable for later units:
  - Do not power the host off (nobody can power it back on remotely);
    test Shut Down by code path only and say so in the report.
  - Do not destroy pre-existing VMs, templates or their disks. Test
    destructive functions on `zz-parity-<unit>-*` objects you created.
  Wait for the host to come back and confirm XenAPI answers before you
  continue or report.
- Do not commit. Leave changes in the working tree and end with the report.
- If blocked for more than ~20 minutes on one obstacle, stop and report
  `BLOCKED` with what you tried.

## Report format (final message)

```
UNIT: <id>
STATUS: DONE | PARTIAL | BLOCKED
CHANGED: <files>
VERIFIED: <what was actually run and observed>
NOT VERIFIED: <what was not>
FOUND, NOT FIXED: <list>
```
