# Closest Visit Selection

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adlb-closest-visit.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** place each laboratory record in its analysis visit
(`AVISIT`, `AWTARGET`, `ADIST`) and flag the record closest to
the visit's target day (`ANL01FL`).

**Input:** one record per laboratory measurement, carrying the
analysis date (`ADT`), the study day (`ADY`, empty when it could
not be computed), and the measured result (`AVAL`).

**Variables:**

- `AVISIT` is the analysis visit the record falls in (`WEEK 2`
  covers study days 8 through 22). It stays empty when the record
  falls outside every window or the study day is missing.
- `AWTARGET` is the study day the visit aims at (day 15 for
  `WEEK 2`). It stays empty when the record falls outside every
  window or the study day is missing.
- `ADIST` is how far the record's study day lies from the target,
  in days and without direction. It stays empty when the record
  falls outside every window or the study day is missing, and it
  shows why a record was chosen: the smallest distance wins.
- `ANL01FL` holds `Y` for the record that stands for its subject
  and parameter in the visit: the closest to the target, the one
  with the later study day when two are equally close, or the one
  with the lower sequence number when they share the same day. It
  stays empty on every other record, and a record outside every
  window or with a missing study day is never flagged.

**Note:** the three window columns travel together: a record
inside the window carries all three, while a record outside every
window or with a missing study day carries none and is never
flagged. All records stay in the output, and the flag identifies
the ones selected for analysis.

**Standard:** ADaM | **Domain:** ADLB
