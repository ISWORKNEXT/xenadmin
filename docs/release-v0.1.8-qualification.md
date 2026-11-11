# Published v0.1.8 qualification

[Release v0.1.8](https://github.com/ISWORKNEXT/xenadmin/releases/tag/v0.1.8) targets `8944bbc6dc64ce6abedf5e569999101d3b942028` and includes the configured snapshot wizard. [Desktop checks](https://github.com/ISWORKNEXT/xenadmin/actions/runs/43014225339) and [Windows/Linux release build](https://github.com/ISWORKNEXT/xenadmin/actions/runs/43014249064) succeeded.

[Artifact verification](audit-evidence-20261111/release-v0.1.8/verification.json) confirms both downloaded archive hashes match SHA256SUMS and GitHub asset digests. Linux SHA256: `252506c60da5b9265db73d67bd806f73b8f8dc26070162cc71c2214e9b4d44e2`. Windows SHA256: `08ddf81fa8281e8e856125e9ef8cf7fc21f4b8cc2dee8b1eba13aa6ae94a4fa2`.

The extracted Linux binary SHA256 `f8b78fa9486770375e37301969960f9029de35b8aa56f720897dfa683e8bc465` matches the [actual native startup manifest](audit-evidence-20261111/release-v0.1.8/linux-smoke/startup.json). [Its connected VM screen](audit-evidence-20261111/release-v0.1.8/linux-smoke/published-linux-connected.png) shows the downloaded published binary reading a real owned VM on `.170`; this smoke check changed no VM resources. Windows runtime was not tested.

Post-clone failure retention commit `b91dbb763` and the subsequent default-SR action are later changes and are not included in this tag.
