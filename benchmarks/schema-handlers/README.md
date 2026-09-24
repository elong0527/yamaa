# Stated Fallbacks

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/schema-handlers.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** derive vital-signs analysis values where each fragile read
states its fallback beside it: an unmapped or absent test code, a
collected result that cannot be read as a whole number, and a subject
with more than one dosing record.

**Input:** one `spec.yaml`. `VS` carries the vital-signs records:
subject, sequence, test code, the result as entered text, and units.
`EX` carries the dosing records: subject, sequence, treatment, dose,
and start date.

**Fallbacks:**

- `PARAM` codes each test: a collected code with no dictionary entry,
  and a record with no collected code at all, both take the stated
  "Other measure" label.
- `AVAL` reads the collected result as a whole number: a result that
  cannot be read that way, such as "ND" or the fractional "98.6", is
  left blank by the stated fallback. A record with no collected result
  is blank because there is nothing to read.
- `LASTDOSE` takes each subject's latest dose: a dosing record with
  no dose never qualifies, the dosed record with the latest start
  date wins, and a subject with no dosed record keeps the dose blank.

**Note:** each fallback is stated beside the read it guards, so the
run completes with the stated answers instead of failing; a read
whose condition never occurs keeps its ordinary value.

**Standard:** ADaM | **Domain:** ADVS
