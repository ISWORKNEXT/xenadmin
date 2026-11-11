# Published v0.1.10 qualification

[Release v0.1.10](https://github.com/ISWORKNEXT/xenadmin/releases/tag/v0.1.10) targets `806ffbc27159c0122bb629669d29e498503730bb`, including CPU topology and scheduling weight. [Desktop checks](https://github.com/ISWORKNEXT/xenadmin/actions/runs/43023565349) and [Windows/Linux release build](https://github.com/ISWORKNEXT/xenadmin/actions/runs/43023630357) succeeded.

[Artifact verification](audit-evidence-20261111/release-v0.1.10/verification.json) confirms both archives match SHA256SUMS and GitHub asset digests. Linux SHA256: `f4fd60c033ecdff4313bde82f100c880025df1619e252f47cd30b0356dfe24ee`. Windows SHA256: `8201cbceaf0ad4ea28888257ea1e5757efc09e35aeb6a49afe32156b3f03c45e`.

The extracted Linux binary SHA256 `3101d6fcbc64fe1391d9761ccc476d47906fe3f00f245e23922976bf8b7e06c4` matches [the actual startup manifest](audit-evidence-20261111/release-v0.1.10/linux-smoke/startup.json). [The published Linux screen](audit-evidence-20261111/release-v0.1.10/linux-smoke/published-linux-template.png) connected to `.170` and displayed an owned template. This smoke read state without mutating it. Windows runtime was not tested.

The subsequent clipboard and system-font correction is not included in this tag.
