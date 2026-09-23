# Lab-Specific Reference Ranges

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-lb-lab-specific-ranges.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** pick the reference range the way a multi-lab study does -- by
lab, test, sex and age band, and by the period the range was in effect --
and flag each result: `LBSTNRLO`, `LBSTNRHI`, and `LBNRIND`.

**Input:** collected results, one row per result carrying the test code,
recorded sex, the collecting lab, birth date, collection date and numeric
result, plus a reference dictionary keyed by lab, test code, sex and age
band, each entry carrying the dates it was in effect, the unit and the
lower and upper limits.

**Variables:**

- `LBTESTCD` is the test code as collected; together with sex, the lab and
  the subject's age at collection it selects the reference entry.
- `SEX` is recorded sex as collected; together with the test code, the lab
  and the subject's age at collection it selects the reference entry.
- `LBNAM` is the collecting lab as collected; different labs may carry
  different ranges for the same test.
- `LBSTRESN` is the numeric result in standard units as collected; missing
  when the result was not collected.
- `LBORRESU` is the reference unit for the lab, test, sex and age-band
  combination in effect on the collection date.
- `LBSTNRLO` is the lower reference limit in standard units from that
  entry.
- `LBSTNRHI` is the upper reference limit in standard units from that
  entry.
- `LBNRIND` is `LOW` when the result is below the lower limit, `HIGH`
  when it is above the upper limit, and `NORMAL` otherwise; blank when the
  result itself is missing.

**Note:** when a lab revises a range, the entry in effect on the collection
date wins -- a result collected on the first day of the new range is judged
against the new limits -- and a subject whose age falls on an age-band
boundary takes the band that starts at that age.

**Standard:** SDTM | **Domain:** LB
