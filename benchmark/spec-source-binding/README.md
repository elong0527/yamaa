# Source Binding

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/spec-source-binding.html) [![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmark/README.md#lifecycle)

**Goal:** show how each source reference resolves: a qualified name reads
its named dataset, and a bare name reads the output columns.

**Input:** one `spec.yaml` declaring two datasets, `DM` with one row per
subject and `VS` with the vital-signs records that drive the output
rows:

- `VS.USUBJID`, `VS.VSTESTCD`, `VS.VSSTRESN`, and `VS.VSSTRESU` read the
  current vital-signs record;
- `DM.SEX`, `DM.AGE`, and `DM.RACE` reach the demographics dataset
  through the shared subject key, and a missing race stays blank;
- `AGEGR1` groups the already-derived `AGE` into `<65` and `>=65`.

**Note:** a qualified reference always names the dataset it reads, while
a bare name always addresses the output columns, so two datasets can
carry the same variable name without colliding.

**Standard:** ADaM | **Domain:** ADVS
