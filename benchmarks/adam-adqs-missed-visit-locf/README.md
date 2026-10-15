# Carry the last score to each missed scheduled visit

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adqs-missed-visit-locf.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** give every subject in the efficacy population a total score
record at each scheduled visit, adding a last observation carried forward
record for each visit missed, with the analysis value (`AVAL`), the
record-type flag (`DTYPE`), and the efficacy population flag (`EFFFL`).

**Input:** one subject-level record per subject carrying the efficacy
population flag, and the collected ADAS-Cog (Alzheimer's Disease
Assessment Scale, cognitive subscale) total scores, each already assigned
to an analysis visit (`AVISITN`).

**Variables:**

- `AVAL`: the collected score on a collected record. On a carried-forward
  record, the score from the closest earlier visit that has one for the
  same subject; empty when the subject has no earlier score.
- `DTYPE`: empty on a collected record; `LOCF` on a record added for a
  missed scheduled visit.
- `EFFFL`: the subject's efficacy population flag, repeated on every
  record.

**Note:** the protocol schedules Weeks 8, 16, and 24 for every subject in
the efficacy population. A scheduled visit is missed when the subject has
no collected score there, and a collected record with an empty score is
left out, so at a scheduled visit it counts as missed. Subjects outside
the efficacy population keep their collected records and gain none, and
baseline is not a scheduled visit, so a missing baseline adds nothing.
The input carries at most one score per subject per visit.

**Standard:** ADaM | **Domain:** ADQS
