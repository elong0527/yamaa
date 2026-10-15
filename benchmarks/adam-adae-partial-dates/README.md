# Partial Date Imputation

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adae-partial-dates.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** one row per adverse event (AE) with an analysis start date
(`ASTDT`) completed from the collected start at whatever precision
it was collected, a flag (`ASTDTF`) marking the dates whose day was
supplied, and a treatment emergence flag (`TRTEMFL`).

**Input:** adverse event records carrying the reported term
(`AETERM`) and the collected start (`AESTDTC`): a full date, a year
and month, a year alone, non-date text, or nothing at all, plus the
first-exposure date (`TRTSDT`) from the subject-level analysis
dataset (ADSL), empty when the subject has no ADSL record.

**Variables:**

- `ASTDT` is the analysis start date. A fully collected date is used
  as it stands. A year and month is completed to the 15th. A year
  alone, non-date text, and a missing value give no analysis date.
- `ASTDTC` is the same analysis date written as text, empty when
  there is no analysis date.
- `ASTDTF` is `D` when the day was supplied to complete the date. It
  is empty when the date was collected in full and when no analysis
  date could be formed, so the flag always matches the date shown.
  In CDISC terms only the day was imputed, which is the level of
  imputation the flag records.
- `TRTEMFL` is `Y` when the analysis start falls on or after
  `TRTSDT`. It stays empty when there is no analysis date, when the
  event started before first exposure, or when the subject's
  first-exposure date is unknown. A supplied day counts exactly as a
  collected one would, so `ASTDTF` tells a reader which flagged
  events rested on a supplied day.

**Note:** a completed date is never placed before first exposure.
When the 15th would fall before `TRTSDT`, the date moves forward to
the exposure date, which the collected month still allows. When the
whole collected month ends before the exposure date, no day it
allows can satisfy that, so the event is left without an analysis
date rather than moved into a month nobody recorded. A fully
collected date is left exactly as collected even when it falls
before `TRTSDT`, because there is nothing about it to choose.

**Standard:** ADaM | **Domain:** ADAE
