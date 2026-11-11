# Release v0.1.6 verification

[v0.1.6](https://github.com/ISWORKNEXT/xenadmin/releases/tag/v0.1.6) publishes commit `2e9d93273f52948d57015315729e15c771458205`. It includes advanced VM operations, common Properties, resource/metadata separation, full copies and session recovery. The subsequently developed Pool options unit is not part of this tag.

- [Desktop checks receipt](audit-evidence-20261111/release-v0.1.6/42999409903.json): formatting, tests, Clippy and build succeeded. The opted-in real-session test is ignored in ordinary CI; its real-host receipt is separate.
- [Release workflow receipt](audit-evidence-20261111/release-v0.1.6/42999458565.json): Windows and Linux builds, packaging and publication succeeded.
- [Published assets](audit-evidence-20261111/release-v0.1.6/release.json): non-draft release, Windows ZIP, Linux tarball and SHA256SUMS.
- [Downloaded archive checksums](audit-evidence-20261111/release-v0.1.6/artifact-checksums.json): both complete archives match SHA256SUMS and GitHub's published SHA-256 digests.
- [Published Linux binary smoke sequence](audit-evidence-20261111/release-v0.1.6/linux-ui/actions.json): the downloaded Linux executable launched and displayed a real `.170` template and live inventory. No template/VM mutation was made in this smoke check.

The [startup manifest](audit-evidence-20261111/release-v0.1.6/linux-startup-ui/startup.json) binds a second smoke sequence to the downloaded executable path, binary SHA-256, process ID and selected template UUID, without recording credentials. The audit driver now records this manifest for future runs.

An initial extraction guard rejected the archive's harmless top-level directory name after the hash and workflow checks had passed. Extraction was retried with that exact root-directory entry accepted and the data extraction filter retained. This was a local verification-script issue, not a failed release build.

Windows compilation and archive integrity are verified; Windows runtime interaction was not tested. The Linux release smoke does not repeat every functional qualification against the optimized artifact. Detailed real-host operation receipts remain in the advanced VM, Properties and full-copy qualification documents. Broader functional parity remains open.
