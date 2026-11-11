# Functional parity completion plan

The user requested the remaining upstream functions, excluding permissions and certificate work. Visual redesign is deferred. The source baseline is upstream `ba43130ffb30d05d1eb40187b8ef9f2de111468a`; the starting local release is `v0.1.5`, commit `46d145afddfb4eca647f4e298cc42de4e5ad053f`.

## Completion rule

Each in-scope capability needs a real backend operation, an accessible native control, state and failure handling, and an observable result verified against a real target. Compilation or a matching button label alone does not complete a capability. Evidence must identify the object and tested state. Cleanup must preserve pre-existing resources.

Statuses are VERIFIED, NOT VERIFIED, INCONCLUSIVE, or EXCLUDED. An unavailable backend, missing device, guest limitation, or missing second host leaves the relevant test NOT VERIFIED or INCONCLUSIVE. It does not convert the feature to VERIFIED.

The baseline contains 299 overlapping capability rows plus command and action crosswalks. This is a multi-day port and qualification effort across the workstreams below; the row count is not an estimate of independent features or completion percentage. Work proceeds in independently verifiable units.

## Scope and sequence

- [x] Read the verification and sequencing principles and frame the run.
- [x] Retain the existing source audit and real-host qualification as the baseline.
- [ ] VM operations: conversion to template, start paused, checkpoint and quiesced snapshot.
- [ ] Live resources: vCPU addition and disk growth, then remaining CPU and boot properties.
- [ ] Storage: disk and SR properties, full-copy and move workflows, discovery and storage backends.
- [ ] Import and export: remaining XVA options, OVF and OVA, disk images and network mapping.
- [ ] Snapshot metadata, lineage, descriptions, screenshots and recovery options.
- [ ] Guest tools, device assignment, VM appliances and startup ordering.
- [ ] Host properties, backup and restore, maintenance and update workflows.
- [ ] Network properties, advanced bond and VLAN handling, SR-IOV and tunnels.
- [ ] Monitoring, graph definitions, alerts, reports and task history controls.
- [ ] Folders, tags, custom fields, search interchange, plugins and application preferences.
- [ ] Pool lifecycle, migration, HA, WLB, PVS and disaster recovery.
- [ ] Whole-product checks and independent audit-trail review.

Permissions, RBAC, AD subjects, privilege elevation, certificate installation/reset, TLS trust policy and security-credential changes are EXCLUDED. Existing read-only guards and connection behavior remain in place.

## Execution and evidence

The first implementation unit extends existing action and dialog paths rather than adding a separate action framework. The live harness captures baseline behavior and then checks the same operations after implementation. It uses temporary VMs on the explicitly authorized `.170` development host, reads values back from XenAPI, and removes owned test resources.

The decision trail is [parity-decisions.tsv](parity-decisions.tsv). Evidence files and capability updates accompany each unit. Mock-host results do not substitute for live qualification. Shared source files have one writer; feature work stays serial until an independent seam justifies delegation.

The known lab has one host and one management PIF. Multi-host and hardware-dependent qualification needs suitable targets. Work on other capabilities continues while those dependencies remain unresolved.

## Common object properties unit

The next shared seam is a typed editor for names, descriptions and tags on VMs (including templates and snapshots), disks, SRs, networks, hosts and pools. A closed object-kind enum selects the XenAPI class; the UI cannot send an arbitrary method or raw record. Reads return only the editable fields. Writes re-resolve UUIDs on the selected connection, validate all input before mutation and update only changed fields. A successful response must include server readback. Existing state guards and unrelated configuration stay intact.

This unit follows completion of advanced VM qualification. Hardware-specific and cross-host operations remain separate workstreams.

## Legacy pool RPC follow-up

The new Pool options owner uses canonical lowercase `pool` RPC names. Legacy HA/WLB and VLAN paths still contain SDK-style uppercase `Pool` prefixes. Their spelling-only correction was deferred after review found that enabling the HA path also activates unsafe ambiguous-error rollback to zero tolerated failures. Resolve the workflow state and recovery rules together before qualifying those paths. The known one-host/one-management-PIF lab does not qualify HA, WLB service integration or management-interface VLAN creation.
