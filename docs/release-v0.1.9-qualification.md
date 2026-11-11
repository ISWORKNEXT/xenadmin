# Published v0.1.9 qualification

[Release v0.1.9](https://github.com/ISWORKNEXT/xenadmin/releases/tag/v0.1.9) targets `fba7f5fc0ec48fee753af34a08aec2f89b2638a9`, including retained failed-creation outputs and Set as default SR. [Desktop checks](https://github.com/ISWORKNEXT/xenadmin/actions/runs/43017090277) and [Windows/Linux release build](https://github.com/ISWORKNEXT/xenadmin/actions/runs/43017190077) succeeded.

[Artifact verification](audit-evidence-20261111/release-v0.1.9/verification.json) confirms both downloaded archive hashes match SHA256SUMS and GitHub asset digests. Linux SHA256: `5db3d7dd8c4163c8c30a6008058656ea027b1d7054fd7c994e4cfcb7780fd811`. Windows SHA256: `416b423c811fc5494745470f864c6b5204811c71a82bc8cfbe8172ef24aee9f4`.

The extracted Linux binary SHA256 `af83d55dd172e14d701d37f071eece859d650007b4b6ba46a7a61d8a8a52be7f` matches the [actual startup manifest](audit-evidence-20261111/release-v0.1.9/linux-smoke/startup.json). [Its real SR screen](audit-evidence-20261111/release-v0.1.9/linux-smoke/published-linux-default-sr.png) displays current pool-default state on `.170`. The smoke action read state and did not change it. Windows runtime was not tested.

The first smoke launch failed before app startup because the owned private Xvfb display had exited with code 0. After confirming no server/socket/lock remained, the private server was restarted and the launch passed. [Exact runtime receipts](audit-evidence-20261111/cpu-settings/qa-runtime-restart.json) retain the failure, old clean exit, restart and new owned PID. The reason for that server exit remains inconclusive.

The subsequent CPU topology/weight unit is not included in this tag.
