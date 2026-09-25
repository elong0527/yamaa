# Assign Analysis Windows by Study Day

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-advs-windows.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** assign each systolic blood pressure (`SYSBP`) record to
its analysis window, adding the analysis visit (`AVISIT`), its
numeric order (`AVISITN`), and `ANL01FL`.

**Input:** vital signs records carrying the collected visit
(`VISIT`, `VISITNUM`), the analysis date (`ADT`), the relative
study day (`ADY`), and the measured value (`AVAL`).

**Variables:**

- `AVISIT` is the analysis visit whose window holds the record's
  study day: `SCREENING` before day 0, `BASELINE` on day 1 (study
  days skip from day -1 to day 1), `WEEK 2` on days 2 through 21,
  `WEEK 4` on days 22 through 42, and `POST-TREATMENT` on day 43
  onward with no upper bound. It is missing when the study day is
  missing, so such a record belongs to no window.
- `AVISITN` is the numeric order of the analysis visit: `-1` for
  `SCREENING`, `0` for `BASELINE`, `2` for `WEEK 2`, `4` for
  `WEEK 4`, and `99` for `POST-TREATMENT`; missing when there is
  no window.
- `ANL01FL` is `Y` on the record that represents its study,
  subject, parameter, and analysis visit: the earliest by study
  day, with the lower sequence number breaking a tie on the same
  day. It is blank on every other record, including any record
  with no study day.

**Note:** windows follow the study day rather than the collected
visit name, so a record the site left unscheduled still belongs
to whichever window its day falls in, and a record on or past
day 43 falls in the open-ended final window. Each window starts
with its first day and ends before the next window's first day,
so day 22 opens Week 4 and day 43 opens post-treatment.

**Standard:** ADaM | **Domain:** ADVS
