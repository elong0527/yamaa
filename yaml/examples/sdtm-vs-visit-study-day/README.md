# Attach visit metadata and study day to a result

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/sdtm-vs-visit-study-day.html)

**Goal:** carry each collected vital-signs result with its test,
result, and collection date, and add the planned visit number, the
trial period, and the study day: `VSTESTCD`, `VSORRES`, `VSDTC`,
`VISITNUM`, `EPOCH`, and `VSDY`.

**Input:** collected vital-signs rows with test, result,
collection date, and visit label; demographics rows carrying the
reference start date; a trial-visits table carrying the planned
number per visit label; and an epoch-range table carrying the
period with its study-day bounds.

**Variables:**

- `VSTESTCD` is the test short name, carried over unchanged.
- `VSORRES` is the result in original units, carried over
  unchanged.
- `VSDTC` is the collection date, carried over unchanged; missing
  when no date was collected.
- `VISITNUM` is the planned visit number from the trial-visits
  table for the collected visit label; missing when the label
  names no planned visit, such as `UNSCHEDULED`.
- `EPOCH` is the trial period from the epoch-range table whose
  range contains the study day: `SCREENING`, `TREATMENT`, or
  `FOLLOW-UP`; blank when there is no study day.
- `VSDY` is the study day of the collection date, counted from the
  subject's reference start date: that date is day 1, there is no
  day zero, and dates before it count back from -1; missing when
  the result has no date or the subject has no reference start
  date.

**Note:** the reference start date is used to count the study day
and then dropped, since it is not part of the result record; the
epoch-range input is an example fixture rather than a standard
trial-design domain.

**Standard:** SDTM | **Domain:** VS
