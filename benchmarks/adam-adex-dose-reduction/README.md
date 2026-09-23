# Dose Reduction Flag

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adex-dose-reduction.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** flag each exposure administration whose dose was reduced
from the previous administration, adding `DOSREDFL` (Dose Reduced
Flag).

**Input:** exposure (EX) records carrying a sequence number
(`EXSEQ`), treatment start date/time (`EXSTDTM`), and collected dose
(`EXDOSE`).

**Variables:**

- `DOSREDFL` is `Y` when the current dose is lower than the previous
  dose for the same subject; empty otherwise, for the first record
  and whenever the current or the previous dose is zero or missing.

**Note:** the comparison runs in chronological treatment-start order
within each subject, breaking timestamp ties by sequence number. The
previous dose is always the administration just before, never an
earlier nonzero dose, so a pause in dosing (a zero dose) is not
flagged and neither is the administration after it.

**Standard:** ADaM | **Domain:** ADEX
