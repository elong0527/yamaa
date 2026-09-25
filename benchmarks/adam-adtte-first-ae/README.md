# Measure Time to the First Adverse Event

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adtte-first-ae.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** build one `TTAE` (Time to First Adverse Event) record
per subject, counting the days from treatment start to the
first adverse event, or to the end of the study when no event
with a usable onset date exists.

**Input:** one row per subject with treatment start
(`TRTSDT`) and end-of-study (`EOSDT`) dates, plus adverse
event records carrying onset dates (`ASTDT`).

**Variables:**

- `STARTDT`: treatment start date; blank when the subject
  never started treatment.
- `ADT`: the earliest adverse event onset date, or the
  end-of-study date when no event carries a usable onset date.
  A date before treatment start is moved up to it; blank when
  neither source date exists.
- `AVAL`: whole days from `STARTDT` through `ADT`, counting
  both endpoints; blank when either date is missing.
- `CNSR`: `0` when the record marks an event, `1` when the
  subject is censored at end of study.
- `EVNTDESC`: `AE` for an event, `END OF STUDY` for a
  censored record.
- `SRCDOM`: `ADAE` for an event, `ADSL` for a censored record.
- `SRCVAR`: `ASTDT` for an event, `EOSDT` for a censored
  record.
- `SRCSEQ`: the chosen event's sequence number: the lower
  number when several events share the earliest onset date;
  blank for a censored record.

**Note:** an event with no onset date cannot start the clock:
it is passed over when a dated event exists, and when it is
the only event the subject is censored at the end of study.
Moving a date up to the treatment start changes only the
date: the record keeps its event-or-censoring status and
source, and the day count is `1`.

**Standard:** ADaM | **Domain:** ADTTE
