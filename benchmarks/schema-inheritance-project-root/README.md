# Shared Input, Project Data

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/schema-inheritance-project-root.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** demonstrate a shared level declaring a study's demographics
input once - its name and the `AGE` field type - while the stored file
stays with the study's project, and derive `AGE` from it.

**Input:** two spec files and the project's demographics:

- `common/spec_common.yaml` is the shared level, kept in its own
  directory as it would be when several studies inherit it. It names
  the demographics file `input/dm.csv`, gives the `AGE` field type,
  and declares every column;
- `spec_study.yaml` is the entry: it names the shared level as its
  parent and declares the complete output;
- `input/dm.csv` sits at the project root, not beside the shared level.

The chain resolves to `expected/spec_resolved.yaml`, the default view in
the specification dropdown.

**Note:** a path is read from the directory of the file that writes it
first, so the resolved chain states it as `common/input/dm.csv`.
Nothing is stored there, so the run reads the same path from the
project root instead, which is where each study keeps its own data.
A file stored beside the shared level would still win.

**Standard:** ADaM | **Domain:** ADSL
