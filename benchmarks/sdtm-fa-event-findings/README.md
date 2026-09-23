# Event Findings

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-fa-event-findings.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** derive `FASEQ`, `FATESTCD`, `FATEST`, `FAOBJ`, `FACAT`,
`FAORRES`, `FAORRESU`, `FASTRESC`, `FASTRESN`, `FASTRESU`,
`FALNKID`, and `FADTC` for findings collected about individual
adverse events on a supplementary form, in Findings About (FA).

**Input:** one row per question per linked adverse event from the
findings form, carrying the link identifier shared with the
adverse event record, the question asked, the recorded answer,
and the collection date; plus the adverse event records, each
carrying the event term under its link identifier.

**Variables:**

- `FATESTCD` is `LOC` for the location record, `SIZE` for the size
  record, and `BIOPSY` for the biopsy record.
- `FATEST` is `Location`, `Size`, and `Biopsied` respectively.
- `FAOBJ` is the event term of the linked adverse event (both
  events here are a rash).
- `FACAT` is always `AE`.
- `FAORRES` is the answer as recorded on the form.
- `FAORRESU` is the size unit, on `SIZE` records only.
- `FASTRESC` copies `FAORRES`.
- `FASTRESN` is the measured size as a number, on `SIZE` records
  only.
- `FASTRESU` is the size unit, on `SIZE` records only.
- `FALNKID` carries the link identifier shared with the linked
  adverse event record, tying each finding to its specific event.
- `FADTC` is the date the findings form was collected.
- `FASEQ` numbers the subject's records by linked event, then by
  test order (`LOC`, `SIZE`, `BIOPSY`).
- An adverse event with no findings form has no FA records.

**Note:** the two rash events on one subject share the same event
term, so `FAOBJ` alone does not distinguish them; `FALNKID` is
what ties each finding to its specific event record.

**Standard:** SDTM | **Domain:** FA
