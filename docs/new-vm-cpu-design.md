# New VM CPU controls

Pinned upstream `Page_CpuMem` separately selects maximum and initial vCPUs when hotplug is supported, and selects cores per socket. Our wizard currently collapses both CPU counts and cannot replace inherited topology. The existing Resources owner already validates and writes all three values.

Keep CPU writes in that owner. Change `CreateVmRequest` to carry explicit `vcpus_max`, `vcpus_at_startup`, and optional `cores_per_socket`; migrate every caller in this unit. Existing audit callers supply equal counts and omit topology. The native wizard supplies explicit selected topology and defaults counts from authoritative source details. Its review page displays both counts and topology. Windows classification reuses existing guest detection; all hosts must explicitly advertise `restrict_set_vcpus_number_live=false` for separate counts. Missing or unknown restriction values remain restricted. For unsupported sources, both defaults use the source startup count, matching upstream. Pre-Ely edition-based fallback is outside this version-qualified unit.

Reject zero/startup-over-maximum and invalid socket arithmetic before cloning. Apply the existing resource update after disk/media and home-server configuration so its physical-host check sees final placement. Failures continue retaining the known cloned output. No creation retry or rollback is introduced.

The alternative was an independent provisioning-only platform setter. That duplicates Resources validation and fresh-map preservation, and can disagree with placement after disk moves. Reusing the existing owner avoids that second path. Preclone topology checks cover arithmetic; final physical-host validation occurs after placement and can leave a retained output on failure. Multi-host placement needs its own real qualification.

Qualification must exercise unequal CPU counts, explicit topology, invalid input without new objects, exact source preservation and actual wizard actions. XenAPI configuration alone does not prove guest-visible socket presentation or runtime hot-add on a new guest.
