# New VM wizard live qualification

The native New VM wizard was exercised against the authorized development host `192.168.8.170` on 2026-11-11. No mock host or fixture test was used in this qualification.

## Implemented behavior

The wizard has six steps: Template, Name, Resources, Storage & media, Network, and Review. Creation runs through workspace task tracking and leaves the VM halted.

The backend distinguishes installation templates from templates with installed disks. Installation templates use their `other_config.disks` specification with the selected SR and first-disk size, followed by `VM.provision`. Installed templates retain their cloned disks and firmware without another provision call. This follows the advice obtained from the user's backend chat `01a0f19d-6bc0-7c13-8870-3aee256d4ccc` and the existing legacy-template qualification implementation in the game-host project.

## Observed results

| Path | Result |
| --- | --- |
| Actual wizard clicks using Other install media | Created VM `036d15b8-0481-2c74-26fb-91baea9a14b7`. Independent XenAPI readback confirmed 2 vCPUs, 256 MiB RAM, a bootable 1 GiB disk, Pool-wide network 0, guest-tools.iso, and Halted power state. |
| Back from Review and forward again | The selected values remained present. |
| Debian Bookworm 12 installation template through the core API | Created a 1 GiB disk from the provisioning specification. UEFI remained unchanged. Source-template disk and boot metadata remained unchanged. |
| Debian 11 Legacy installed template through the core API | Retained a separate 10 GiB cloned root disk and BIOS firmware. Source-template disk and boot metadata remained unchanged. |
| Host rejection after disk provision | A deliberately invalid vCPU value caused a host error. The corrected rollback removed the temporary VM and disk. |

The rollback test exposed a cleanup bug: `VM.provision` creates writable VDIs with type `system`, while the public detached-disk delete operation accepts only type `user`. Rollback now removes only detached, writable `user` or `system` disks collected from the newly created VM, after excluding source-template disks. The general disk-delete restriction remains in place.

The first failed rollback left one identified 1 GiB disk. That test disk was removed after its UUID, description, size, and detached state were checked. A second live failure test passed with the corrected code.

All test VMs and their disks were removed. The final VDI reference list exactly matched the baseline taken before the failure tests. The shared ISO remained present. The independent final readback is recorded in [cleanup.json](audit-live-ui-evidence-20261111/new-vm/cleanup.json).

Screenshots show the [Review step](audit-live-ui-evidence-20261111/new-vm/wizard-review.png) and the [completed creation task](audit-live-ui-evidence-20261111/new-vm/wizard-task-final.png). The GUI ran under a nested Xwayland display to support input automation against the real native application.

## Qualification limits

These tests cover creation and rollback. They do not establish successful OS installation, guest IP or password configuration, guest boot, multi-host placement, or disk moves between SRs. The selected guest-tools.iso verified media attachment rather than OS installation. This change does not establish complete XCP-ng Center parity.
