# Sum floating-point values in source record order

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-adlb-order-sensitive-sum.html)

**Goal:** add up one subject's laboratory values in the order the
source records were stored.

**Input:** collected laboratory (LB) results with study, subject, and
sequence keys carried through, plus the numeric result (`LBSTRESN`)
for each record.

**Variables:**

- `AVAL` is the collected analysis value for the record.
- `AVALSUM` is the subject's values added in their stored record
  order and carried onto every record for that subject.

**Note:** binary floating-point addition makes the total sensitive
to that order: adding `0.1`, `0.2`, and `0.3` gives `0.6000000000000001`,
while adding the same values in reverse gives `0.6`.

**Standard:** ADaM | **Domain:** ADLB
