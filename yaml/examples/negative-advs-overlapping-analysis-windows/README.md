# Reject overlapping analysis windows

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-advs-overlapping-analysis-windows.html)

**Goal:** give each analysis record the visit of the one window
whose first and last analysis days surround the record's analysis
day (`ADY`).

**Input:** analysis records carrying the analysis day (`ADY`), and
one study-wide window table giving each visit its first (`AWLO`)
and last (`AWHI`) analysis day for the same study.

**Variables:**

- `ADY`: the record's analysis day, carried through as given. The
  row would contain this day and the visit of the window whose
  range holds it, but a day falling in two windows has no single
  visit, so no row is produced.

**Note:** the run fails because two windows contain the same day,
and choosing either visit would depend on a rule the study did
not state.

**Standard:** ADaM | **Domain:** ADVS

## How to fix

First correct unintended overlap in the window table. If overlap
is intentional, state and justify a deterministic selection
policy with `order_by` and `keep`; do not rely on source-row
order.
