# Cumulative Dose

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adex-cumulative-dose.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** summarize exposure for each planned subject-treatment,
carrying `EXTRT` (treatment name) and `EXDOSU` (dose units) through
and adding `DOSECUM` (cumulative dose), `NCYCLES` (cycle count), and
`RDI` (relative dose intensity).

**Input:** planned treatments with dose units and planned totals,
plus exposure records holding administered doses.

**Variables:**

- `DOSECUM` is the total administered dose across the subject's
  exposure records for the treatment. A record with no collected
  dose adds nothing; a planned treatment with no exposure records
  has an empty total.
- `NCYCLES` is the number of exposure records for the treatment.
  Every record counts, even one with a zero or missing dose. A
  planned treatment with no exposure records has an empty count.
- `RDI` is cumulative dose as a percentage of the planned total
  dose across its cycles. It is empty when the planned total is
  zero or when there is no cumulative dose to compare.

**Note:** an administered zero dose adds nothing to the total but
its record still counts, so a treatment given only as zero doses
has a zero total alongside a nonzero count. A duplicated exposure
record counts once per entry, so its dose enters the total twice.

**Standard:** ADaM | **Domain:** ADEX
