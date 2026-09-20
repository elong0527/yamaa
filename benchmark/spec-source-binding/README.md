# Source Binding

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/spec-source-binding.html) [![Lifecycle: finalized](https://img.shields.io/badge/Lifecycle-finalized-brightgreen)](https://github.com/elong0527/yamaa/blob/main/benchmark/README.md#lifecycle)

**Goal:** show how each source reference resolves: a qualified name
reads its named dataset, and a bare name reads the output columns.

**Input:** one `spec.yaml` declaring two datasets. `VS` carries the
vital-signs records that drive the output rows: subject (`USUBJID`),
sequence (`VSSEQ`), test (`VSTESTCD`), result (`VSSTRESN`), and units
(`VSSTRESU`). Each subject's sequence starts at 1, the SDTM way.
`DM` carries one row per subject with sex, age, and race.

**Bindings:**

- `VS.USUBJID`, `VS.VSSEQ`, `VS.VSTESTCD`, `VS.VSSTRESN`, and
  `VS.VSSTRESU` read the current vital-signs record straight through;
  the sequence restarts at 1 for each subject and survives into
  `ADVS` as `ASEQ`, a key beside the subject;
- `DM.SEX`, `DM.AGE`, and `DM.RACE` reach the demographics dataset
  through the shared subject key, and a missing race stays blank;
- `AGEGR1` groups the already-derived `AGE` into `<65` and `>=65`;
  the bare name binds to the output column, never to a source dataset.

**Note:** a qualified reference always names the dataset it reads, so
`DM` and `VS` may carry same-named variables without colliding; a bare
name always addresses the output columns, so one column can build on
an earlier column.

**Standard:** ADaM | **Domain:** ADVS
