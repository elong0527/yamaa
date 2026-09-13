# Summarize cumulative exposure

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-adex-cumulative-dose.html)

**Goal:** summarize exposure for each planned subject-treatment,
carrying `EXTRT` (treatment name) and `EXDOSU` (dose units) through
and adding `DOSECUM` (cumulative dose), `NCYCLES` (cycle count), and
`RDI` (relative dose intensity).

**Input:** planned treatments with dose units and planned totals,
plus exposure records holding administered doses.

**Variables:**

- `DOSECUM` is the total administered dose across the subject's
  exposure records for the treatment.
- `NCYCLES` is the number of exposure records for the treatment.
- `RDI` is cumulative dose as a percentage of the planned total
  dose across its cycles; empty when the planned total is zero.

**Note:** an administered zero dose adds nothing to the total but its
record still counts, so a treatment given only as zero doses has a
zero total alongside a nonzero count.

**Standard:** ADaM | **Domain:** ADEX
