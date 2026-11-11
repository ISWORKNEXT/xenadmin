# Published v0.1.7 qualification

[Release v0.1.7](https://github.com/ISWORKNEXT/xenadmin/releases/tag/v0.1.7) targets commit `1060cd5471e051fd42a369c9ee12d0a44ffe850f`, including the Pool options unit. Both [desktop checks](https://github.com/ISWORKNEXT/xenadmin/actions/runs/43004455596) and [Windows/Linux release build](https://github.com/ISWORKNEXT/xenadmin/actions/runs/43004519286) succeeded.

The reproducible [artifact verifier](../tools/audit_release_artifacts.py) downloaded both archives and compared actual SHA256 values with SHA256SUMS and the published asset digests. [Verification receipt](audit-evidence-20261111/release-v0.1.7/verification.json), release metadata and workflow JSON are retained together.

| Archive | SHA256 |
| --- | --- |
| Linux x86_64 tar.gz | `78cedd7cf8bc2d2f742dd2f711488f7916904cc6de3c82444ff171a2a2328f43` |
| Windows x86_64 zip | `3f518b37f636969bf749e6dae7a0637ea1afad709a0ee4a82abba60db7847249` |

The extracted Linux binary SHA256 is `3fafe864d283620b43cf94b2b5755b4e520016a4a4277dc7a12df9e4fc0831b4`. The [native startup manifest](audit-evidence-20261111/storage-actions/native-ui/startup.json) records that exact binary connecting to real `.170`. [Storage qualification](storage-actions-live-qualification-20261111.md) confirms its actual Full copy action to another writable SR and full disk checksum readback. Windows built and published successfully; Windows runtime was not tested.
