# Visit Study Day

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-vs-study-day.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** carry each collected vital-signs result with its test,
result, and collection date, and add the planned visit number, the
trial period, and the study day: `VSTESTCD`, `VSORRES`, `VSDTC`,
`VISITNUM`, `EPOCH`, and `VSDY`.

**Input:** collected vital-signs rows with test, result,
collection date, and visit label; demographics rows carrying the
reference start date; a trial-visits table carrying the planned
number per visit label; and an epoch-range table (a benchmark
fixture rather than a standard trial-design domain) carrying each
period with its study-day bounds.

**Variables:**

- `VISITNUM` is the planned visit number from the trial-visits
  table for the collected visit label; missing when the label
  names no planned visit, such as `UNSCHEDULED`.
- `EPOCH` is the trial period from the epoch-range table whose
  range, both bounds included, contains the study day: `SCREENING`,
  `TREATMENT`, or `FOLLOW-UP`; blank when there is no study day or
  no range contains it.
- `VSDY` is the study day of the collection date, counted from the
  subject's reference start date: that date is day 1, there is no
  day zero, and dates before it count back from -1; missing when
  the result has no date or the subject has no reference start
  date.

**Note:** the visit number follows the visit label while the epoch
follows the collection date, so an unscheduled visit still falls in
an epoch, and a planned visit with no collection date has a visit
number but no epoch.

**Standard:** SDTM | **Domain:** VS
