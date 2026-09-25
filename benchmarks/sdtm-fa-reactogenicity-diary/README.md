# Reactogenicity Diary

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-fa-reactogenicity-diary.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** derive `FASEQ`, `FATESTCD`, `FATEST`, `FAOBJ`, `FACAT`,
`FASCAT`, `FAORRES`, `FAORRESU`, `FASTRESC`, `FASTRESN`,
`FASTRESU`, `FASTAT`, `FATPT`, and `FADTC` for each solicited
reaction on a vaccine reactogenicity diary in Findings About (FA).

**Input:** one row per diary day per solicited reaction, carrying
the diary day label, the collection date, the reaction, whether the
reaction is local or systemic, the occurrence and severity recorded
for completed days, the measured diameter for redness and swelling,
and whether the diary day was completed.

**Variables:**

- `FASEQ` numbers the subject's records by diary day, reaction name,
  and test order (`OCCUR`, `SEV`, `LDIAM`). Output records are grouped
  by subject, then ordered by that number.
- `FATESTCD` is `OCCUR` for the occurrence record, one per reaction
  per diary day; `SEV` for the severity record, on completed days
  only; and `LDIAM` for the longest-diameter record, only for redness
  or swelling that occurred on a completed day.
- `FATEST` is `Occurrence Indicator`, `Severity/Intensity`, and
  `Longest Diameter` respectively.
- `FAOBJ` copies the solicited reaction.
- `FACAT` is always `REACTOGENICITY`.
- `FASCAT` is `ADMINISTRATION SITE` for local reactions and
  `SYSTEMIC` for systemic reactions.
- `FAORRES` is the recorded `Y`/`N` for `OCCUR`, the recorded
  severity for `SEV` (`NONE` unless the reaction occurred), and the
  measured diameter for `LDIAM`; it is blank on a missed diary day.
- `FAORRESU` is the diameter unit, on `LDIAM` records only.
- `FASTRESC` copies `FAORRES`.
- `FASTRESN` is the measured diameter as a number, on `LDIAM`
  records only.
- `FASTRESU` copies the diameter unit, on `LDIAM` records only.
- `FASTAT` is `NOT DONE` for a missed diary day, and blank
  otherwise.
- `FATPT` is the diary day label (`END DAY 1`, ...).
- `FADTC` is the diary collection date.

**Note:** temperature stays in Vital Signs (VS) and is not a
reactogenicity diary record, so it never becomes an FA record here.

**Standard:** SDTM | **Domain:** FA
