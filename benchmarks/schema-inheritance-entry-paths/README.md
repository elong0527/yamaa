# Inherited Input, Study Data

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/schema-inheritance-entry-paths.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** demonstrate a shared level declaring a study's demographics
input once - its name and the `AGE` field type - while each study keeps
the stored file beside its own entry, and derive `AGE` from it.

**Input:** two spec files and the study's own demographics:

- `common/spec_common.yaml` is the shared level, kept in its own
  directory as it would be when several studies inherit it. It declares
  the `DM` input as `input/dm.csv` with `relative_to: entry`, the `AGE`
  field type, and every column;
- `spec_study.yaml` is the entry: it names the shared level as its
  parent and declares the complete output;
- `input/dm.csv` sits beside the entry, not beside the shared level.

The chain resolves to `expected/spec_resolved.yaml`, the default view in
the specification dropdown.

**Note:** a path is normally read from the directory of the level that
writes it, so the inherited `input/dm.csv` would name
`common/input/dm.csv`, which does not exist. `relative_to: entry` reads
it from the entry's directory instead, so every study that inherits the
shared level reads its own file. The resolved chain states the path as
the study reads it and no longer carries `relative_to`.

**Standard:** ADaM | **Domain:** ADSL
