# Flag the safety and intent-to-treat populations

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adsl-population-flags.html) [![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmark/README.md#lifecycle)

**Goal:** flag each subject for the safety population (`SAFFL`)
and the intent-to-treat (ITT) population (`ITTFL`).

**Input:** subject-level data carrying the planned arm (`ARMCD`)
and the treatment start date (`TRTSDT`), either of which may be
missing.

**Variables:**

- `SAFFL` is `Y` when `TRTSDT` is present and `N` when it is
  missing, marking the subjects who received any treatment.
- `ITTFL` is `Y` when `ARMCD` is present and `N` when it is
  missing, marking the subjects who were randomized.

**Note:** the two flags are independent: a subject can be
randomized without being treated, and each flag rests on the one
fact that justifies it.

**Standard:** ADaM | **Domain:** ADSL
