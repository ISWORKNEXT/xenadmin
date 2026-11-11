# Pool VM autostart

Pinned ChangeHostAutostartAction writes pool.other_config:auto_poweron; Host.SetVmAutostartEnabled delegates to the pool extension. This is one pool-wide flag, distinct from each VM's auto_poweron flag. Reuse the existing typed PoolOptions owner and dialog, and expose it from Host actions as well as the pool screen. The dialog must state the pool scope and the requirement for per-VM flags.

Add one boolean selected from the pool map using upstream's literal-true rule. Original/edited intent preserves untouched legacy or absent values. Prevalidate all existing scalar edits first; when autostart changes, merge its one key into a fresh pool map and read back. Retain partial-save error guidance; this is not an atomic transaction or same-key compare-and-swap.

The alternative was a host setter. That would misrepresent the actual owning pool and repeat the legacy remove/add gap. A fresh map write keeps unrelated keys without temporarily removing the flag. Existing HA/WLB and VLAN paths are untouched.

Real qualification requires a journal of the exact original key presence/value before writes, read-only rejection, idempotence, preservation of unrelated state, actual Host actions/dialog Save and raw readback, then exact restoration. Only .170 may be mutated. No host reboot, guest shutdown or failover is part of this configuration check; startup behavior remains a separate runtime qualification.
