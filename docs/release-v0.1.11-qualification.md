# Published v0.1.11 qualification

[Release v0.1.11](https://github.com/ISWORKNEXT/xenadmin/releases/tag/v0.1.11) targets `544ebd970d4e31dedbc4a95077deb66408035807`, including OS clipboard and system font integrations and the corrected X11 CI runtime dependency. [Checks](https://github.com/ISWORKNEXT/xenadmin/actions/runs/43031298486) and [Windows/Linux release build](https://github.com/ISWORKNEXT/xenadmin/actions/runs/43031703816) succeeded.

[Artifact verification](audit-evidence-20261111/release-v0.1.11/verification.json) confirms both archives match SHA256SUMS and GitHub asset digests. Linux SHA256: `c8018757d4b49caf7a9dddbd3fa7290b8e3a14fd3121c3de7d5762ade3c09591`. Windows SHA256: `1b6f9e1d28454151c043ced0fa26434f0b5c2ab018e957bf6211f3695462c79d`.

The extracted Linux binary SHA256 `1afb09fd6c192d7af4058f058a1a13c17feb5f33f7456dbe3b9ea5745b42934c` matches the startup manifests for both smoke checks. [The actual downloaded binary passes exact Unicode OS Copy/Paste](audit-evidence-20261111/release-v0.1.11/linux-clipboard/verdict.json) without credentials or submitting a connection. [Its pasted field](audit-evidence-20261111/release-v0.1.11/linux-clipboard/after-paste.png) renders Thai on this machine. [A subsequent real host view](audit-evidence-20261111/release-v0.1.11/linux-smoke/published-linux-host.png) connects to `.170` and displays XCP-ng8.3.0. The smoke only reads state. Both process cleanup receipts confirm owned app/clipboard children exited.

Windows runtime, native Wayland, IME composition and fonts on other machines remain unqualified. [The clipboard qualification](clipboard-live-qualification-20261111.md) retains the original red runs, the hosted runtime-library failure and successful CI retry.

The subsequent New VM CPU selection unit is not included in this tag.
