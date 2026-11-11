# Failed creation retains its output — 2026-11-11

Configured VM creation and the simple snapshot-output path no longer invoke automatic post-clone failure deletion. Their errors include the known output UUID and state that the VM/object and disks were retained for inspection before another creation attempt. The former cleanup helper inferred disposable disks from all current nonsource attachments; that did not prove ownership when another client attached a disk.

## Real result

[The raw attempt output](audit-evidence-20261111/creation-retention/attempt1-output.txt) runs actual template/copy/storage/snapshot configuration paths on owned small disks in the authorized `.170` lab. The successful configured snapshot case also executes the strengthened exact network, home-host, CPU and three-memory-field assertions.

A subsequent request with `vcpus=u64::MAX` passes local creation preflight, clones the snapshot, then receives host HTTP 500 during post-clone configuration. The retained error identifies VM `ec54b151-2157-db16-7d4a-0a6e7e00590d`. The harness independently finds it in inventory and reads disk `26f0dae6-0c5f-4dce-b702-947516e2033a`: the complete 32 MiB SHA256 remains `3d340b5a4dd59c3dfcfbaefb475dbd2ec82902a34ee27ad31a2c6dcd057f52fb`, equal to the source. The observed exception is HTTP 500; no more specific host-side validation error was established.

The core trace's final process result is **exit 1**, because the immediate cleanup inventory contained one additional VDI `6271c8e9-9c72-47c8-8dd6-e681b28e22a6`. All other original sets matched and no baseline VDI was missing. The following [read-only identity lookup](audit-evidence-20261111/creation-retention/extra-vdi-lookup-receipt.json) returned `UUID_INVALID` before any deletion; the record had disappeared independently. [A fresh full inventory comparison](audit-evidence-20261111/creation-retention/settled-inventory.json) proves all five sets exactly match their baseline: 68 VMs, 81 VDIs, 5 SRs, 5 PBDs and 2 networks. No unidentified VDI was manually deleted. This sequence is consistent with asynchronous storage garbage collection; that explanation is an inference, not a captured host-GC trace.

The harness now allows bounded read-only settling only when all four other sets match and every baseline VDI still exists. Missing baseline disks or other object drift fails immediately; extra VDIs still fail after the bound. This polling extension was compile-checked, not live re-exercised. The original failed immediate comparison remains retained as evidence.

## Limits

The configured-snapshot failure path was exercised. The simple snapshot-output branch was source-reviewed; it was not induced to fail live in this unit. Failures before a clone UUID can be recovered are outside the UUID-reporting guarantee. No durable interrupted-creation reconciliation or automatic retry was added. Successful-path ownership assignment and concurrent foreign-disk changes remain separate work; this fix removes destructive failure cleanup rather than qualifying every concurrent mutation case.
