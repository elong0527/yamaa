# Standardize Non-Numeric Lab Results

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-lb-character-results.html) [![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** carry `LBORRES` into a standardized `LBSTRESC`, populate
`LBSTRESN` for true numerics only, and state `LBNRIND` for results
at the quantification limits.

**Input:** collected laboratory results mixing urine dipstick grades
in several spellings with chemistry results at the lower and upper
quantification limits.

**Variables:**

- `LBTESTCD` is the test short name as collected: `PROT`
  (protein), `GLUC` (glucose), `KETON` (ketones), `CREAT`
  (creatinine), or `CK` (creatine kinase).
- `LBTEST` is the test name as collected.
- `LBORRES` is the result as collected: a dipstick grade such as
  `Neg`, `tr`, or `1 plus`, or a censored value such as `<0.1`.
- `LBSTRESC` is the standardized character result: known dipstick
  spellings fold to `NEGATIVE`, `TRACE`, `1+`, or `2+`; anything
  else is kept as collected.
- `LBSTRESN` is the standardized numeric result. It holds a number
  only for a true numeric such as `0.9`; it stays empty for grades
  and for censored values.
- `LBSTRESU` is the standard unit, carried from `LBORRESU`.
- `LBNRIND` is the reference range indicator: `LOW` for a result
  below the quantification limit, `HIGH` for one above it, empty
  otherwise.
- `LBDTC` is the specimen collection date as collected.

**Note:** a grade spelling outside the known spellings is kept as
collected rather than dropped, so a new spelling shows up in the
output instead of silently disappearing.

**Standard:** SDTM | **Domain:** LB
