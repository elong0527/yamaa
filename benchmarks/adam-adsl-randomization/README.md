# Derive Randomization Timing

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adsl-randomization.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** one record per subject (`STUDYID`, `USUBJID`) recording
when randomization happened: the collected date (`RANDDT`), the
study day of randomization (`RANDDY`), and the collected moment
written as text (`RANDDTC`).

**Input:** demographics (DM) records carrying the subject
reference start date (`RFSTDTC`), the collected randomization
date (`RANDDT`), and the collected randomization moment
(`RANDDTTM`). The sample covers the study-day edges: a
randomization on the reference date (day `1`), one the day
before it (day `-1`), and one several days before it; a subject
never randomized; a collected date with no collected moment; a
missing reference date; and a collected moment with no
collected date.

**Variables:**

- `RANDDT` is the randomization date as collected. It stays
  empty when no randomization date was collected.
- `RANDDY` is the study day of randomization measured from
  `RFSTDTC`: the reference date itself is day `1`, later dates
  count forward inclusively, and earlier dates count backward
  with no day `0`, so the day before the reference date is day
  `-1`. It stays empty when either date is missing.
- `RANDDTC` is the collected randomization moment (`RANDDTTM`)
  written as text at whole-second precision, so a collected
  `10:30` becomes `10:30:00`. It stays empty when no moment was
  collected.

**Note:** the moment is written from what was collected,
independently of the date: a subject with a collected moment but
no collected date still gets `RANDDTC`, while `RANDDT` and
`RANDDY` stay empty.

**Standard:** ADaM | **Domain:** ADSL
