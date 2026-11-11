# VM resource and metadata ownership

## Work phases

- [x] Ground the existing editors, callers and recorded constraints.
- [x] Sketch two structurally distinct designs.
- [x] Agree by internal synthesis; no human checkpoint requested.
- [x] Implement the selected design and migrate its callers.
- [x] Scrap check: assess friction and remove rejected scaffolding.

Arena phases: Frame, Fan out, Cross-judge, Pick and Graft complete; Verify complete for the scoped migration. Configured architect runners and cross-judge: gpt-6.1-sol, medium; two design directions get isolated output directories.

## Problem and contract

The new common property editor supports long descriptions and exact tag values. The existing VM resource form still validates descriptions at 1024 characters, flattens tags into a comma-separated string, and always sends metadata alongside resource settings. An owned real VM now has a 60-line description and a tag with leading/trailing spaces, making the integration observable on the dev host.

The design must make CPU, memory and boot edits preserve unrelated name/description/tag values, including concurrent external metadata edits. It must retain provisioning and existing live-resource state guards, keep functional access to metadata, remove competing metadata writers in this path, and migrate all affected callers in one change. It must not add mocked product behavior, permission work or certificate work.

## Arena rubric

Score each candidate 0–2 for each criterion:

1. Metadata has one owner and one invariant set; unrelated values survive resource edits.
2. The public interface prevents resource callers from accidentally resubmitting metadata.
3. Provisioning, native commands, workspace and live harness callers have concrete migration paths.
4. UI access and live CPU/memory/boot behavior remain reviewable and usable.
5. The design is smaller than its alternatives and can be verified with real `.170` behavior.

Grounding, usage sketches, synthesis and verification evidence follow below.

## Grounding outcome

The old combined API and unconditional tag setter were introduced before the common Properties owner. Commits explicitly require live-memory guards, boot-map preservation, metadata access, inventory/details refresh and reachable Save controls. No reviewed source explains the old combined boundary or numeric limits. GitHub issue searches returned no records. External historical coverage is incomplete; the current ownership requirement is explicit in this run.

Real baseline: resource Save on the owned 60-line-description VM failed at the legacy 1024-character check before changes. Independent metadata hashes stayed unchanged. The retained screenshot and readback will be compared with a successful resource edit after migration. Clone/provisioning description validation must also interoperate with values accepted by common Properties.

## Usage and shape

The selected shape is a dedicated Resources dialog and the existing common Properties dialog. Resource state and commands contain CPU, memory and boot values only. Metadata changes continue through `ObjectPropertiesEdit { original, edited }`.

```rust
let resources = VmResourceUpdate {
    vcpus_max: 4, vcpus_at_startup: 3,
    memory_static_max_mib: 1024,
    memory_dynamic_min_mib: 1024, memory_dynamic_max_mib: 1024,
    boot_order: None, firmware: None, secure_boot: None,
    boot_policy: None, after_crash: None,
};
workspace.update_vm_resources(&connection_id, &vm_uuid, resources).await?;
```

The public resource aggregate has those ten fields and no name, description or tags. Connection accepts `(&str, &VmResourceUpdate)`; Workspace accepts connection ID, VM UUID and the aggregate, retaining membership/lifecycle/refresh policy. Native parses its resource inputs into that aggregate and sends one resource command. Provisioning asks common Properties to apply intended child metadata, then configures resources inside its existing cleanup envelope. Resource completion obtains names/titles from refreshed inventory, never from a submitted resource draft.

A crate-private `validate_description_change(original, desired)` is shared by common Properties and source-aware creation preflight. It validates changed descriptions against the existing common limit while preserving unchanged inherited legacy values. Creation-name validation stays in its existing boundary. No synthetic source-target metadata edit is needed for preflight.

## Synthesis decision

Both isolated candidates were read in full and screened for shallow interfaces, split ownership, compatibility paths, wire leakage and extra lifecycle state. Candidate 1 keeps a combined tabbed dialog with separate saves; candidate 2 uses dedicated Resources and common Properties. Root and the configured independent cross-judge agree:

| Criterion | Combined | Dedicated |
| --- | --- | --- |
| One metadata owner | 2 | 2 |
| Resource payload excludes metadata | 2 | 2 |
| Concrete caller migration | 2 | 2 |
| Usable access and retained guards | 2 | 2 |
| Small design and real verification | 1 | 2 |
| Total | 9/10 | 10/10 |

Candidate 2 is the base. Graft candidate 1's narrow description-change helper rather than the broader complete-metadata validator proposed for provisioning. Reject combined modal lifetime coordination, a combined Save facade, legacy metadata setters/wrappers and unrelated resource-concurrency redesign. Metadata access is already available; the user did not require one editing window.

## Tradeoffs accepted

- Two explicit editing actions keep one metadata experience and avoid coupled drafts/saves.
- Existing scalar resource representations and all resource guards stay intact during ownership migration.
- Provisioning remains sequential with existing cleanup; this does not add a transaction that XenAPI cannot guarantee.

## Verification and next implementation

Migrate all product and live-harness callers in the same change, delete old update/tag routes, and run the real long-description resource edit that failed before. Compare exact metadata before/after a successful CPU/boot change and after an external metadata edit while Resources is open. Retain live CPU/memory and boot-map guards, test source-aware long-description provisioning and clean owned resources. No checkpoint or permission question is required for this authorized internal implementation.

## Implementation result

All old resource/metadata combined routes and unconditional tag setters were removed. The selected aggregate crossed the existing boundaries without compatibility wrappers or additional modal lifetime state. No scrap was needed; both rejected sketches stay outside the product tree. A reviewer found an inherited absent-boot-option mismatch: CPU-only updates could insert secureboot=false. Normalization now occurs only on an explicit firmware switch to BIOS. Real CPU-only and boot-order changes preserved the other map keys.

Native long-description resource saves and external metadata preservation passed; provisioning passed changed 2,000-character and inherited 40,000-character descriptions, exact inherited spaced tags, and rejection before clone for changed oversized text. Sources: advanced-vm/resource-owner-ui/actions.json, property-ui-readback.jsonl and verify-provisioning.jsonl under docs/audit-evidence-20261111. Whole-product parity remains open.
