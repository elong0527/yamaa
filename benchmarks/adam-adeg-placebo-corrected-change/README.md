# Placebo-Corrected ECG Change from Baseline

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adeg-placebo-corrected-change.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** add the time-matched placebo comparator of a placebo-controlled
parallel-group electrocardiogram (ECG) study given in dose cohorts: flag the
placebo records (`ACOMPFL`), name the placebo group each record is compared
with (`COMPTYPE`), carry that group's means (`COMP`, `COMPBASE`, `COMPCHG`),
and correct each drug record's change from baseline by the placebo group's
mean change (`CCOMPCHG`, the double delta).

**Input:** one record per subject, parameter, analysis period (`APERIOD`),
nominal day within the period (`APERDAY`) and nominal time point (`ATPT`,
with its number of hours after dosing in `ATPTN`), each holding the
average of the replicate ECGs taken at that time (`AVAL`, unit `AVALU`),
the subject's dose cohort (`COHORT`) and actual treatment (`TRTA`), and
its time-matched baseline already in place (`ABLFL`, `BASE`, `CHG`,
`BASETYPE`). The day before dosing is the baseline day; every subject is
dosed once.

**Variables:**

- `ACOMPFL`: `Y` on every placebo record after baseline; no value on drug
  records and on baseline records.
- `COMPTYPE`: "Placebo group at " followed by the time point, for example
  "Placebo group at 2 h", on every record after baseline, drug and placebo
  alike, also when no placebo record exists at that time; no value on
  baseline records.
- `COMP`: the mean `AVAL` of the placebo group: every placebo record after
  baseline of the same parameter at the same nominal time point, from all
  cohorts together. No value when the group has no record, and on baseline
  records.
- `COMPBASE`: the mean `BASE` of the same placebo group, over the records
  that have one; no value when none has one, and on baseline records.
- `COMPCHG`: the mean `CHG` of the same placebo group, over the records
  that have one, so not always `COMP` minus `COMPBASE`; no value when none
  has one, and on baseline records.
- `CCOMPCHG`: `CHG` minus `COMPCHG` on drug records after baseline; no
  value on placebo and baseline records, or when either value is missing.

**Note:** a placebo record carries its group's means, not its own values,
and none of the comparator values is rounded.

**Standard:** ADaM | **Domain:** ADEG
