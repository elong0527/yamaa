# Standardize Non-Numeric Lab Results

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-lb-character-results.html) [![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** carry `LBORRES` into a standardized `LBSTRESC`, populate
`LBSTRESN` for true numerics only, and state `LBNRIND` for results
at the quantification limits.

**Input:** long-form ODM data with one row per recorded item. Each
laboratory result is one form occurrence carrying the reported
result (`IT.LB.RESULT`), the collection date (`IT.LB.LBDTC`), and
the reported unit (`IT.LB.LBORRESU`); the collection form names the
test. A repeated form occurrence keeps a second result for the same
test and visit separate (the two creatinine records).

**Variables:**

- `LBTESTCD` is the test short name for the collection form:
  `PROT` (protein), `GLUC` (glucose), `KETON` (ketones), `CREAT`
  (creatinine), or `CK` (creatine kinase).
- `LBTEST` is the test name for the collection form.
- `LBORRES` is the result as collected: a dipstick grade such as
  `Neg`, `tr`, or `1 plus`, or a censored value such as `<0.1`.
- `LBSTRESC` is the standardized character result: known dipstick
  spellings fold to `NEGATIVE`, `TRACE`, `1+`, or `2+`; anything
  else is kept as collected.
- `LBSTRESN` is the standardized numeric result. It holds a number
  only for a true numeric such as `0.9`; it stays empty for grades
  and for censored values.
- `LBSTRESU` is the standard unit, carried from the reported unit.
- `LBNRIND` is the reference range indicator: `LOW` for a result
  below the quantification limit, `HIGH` for one above it, empty
  otherwise.
- `LBDTC` is the specimen collection date from the same form
  occurrence.
- `LBSEQ` numbers the records in the documented collection order
  within each subject.

**Note:** a grade spelling outside the known spellings is kept as
collected rather than dropped, so a new spelling shows up in the
output instead of silently disappearing. No demographics input is
needed: the subject identifier comes from the ODM subject key.

**Standard:** SDTM | **Domain:** LB
