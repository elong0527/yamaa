# Flag laboratory results against reference ranges

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/sdtm-lb-reference-range-indicator.html)

**Goal:** attach the reference unit and limits to each collected
test result and flag the result against them: `LBORRESU`,
`LBSTNRLO`, `LBSTNRHI`, and `LBNRIND`.

**Input:** collected results, one row per result carrying the test
code, recorded sex, and numeric result, plus a reference dictionary
keyed by test code and sex giving the unit and the lower and upper
limits.

**Variables:**

- `LBTESTCD` is the test code as collected; together with sex it
  selects the reference entry.
- `SEX` is recorded sex as collected; together with test code it
  selects the reference entry.
- `LBSTRESN` is the numeric result in standard units as collected;
  missing when the result was not collected.
- `LBORRESU` is the reference unit for the test-and-sex
  combination.
- `LBSTNRLO` is the lower reference limit in standard units.
- `LBSTNRHI` is the upper reference limit in standard units.
- `LBNRIND` is `LOW` when the result is below the lower limit,
  `HIGH` when it is above the upper limit, and `NORMAL` otherwise;
  blank when the result itself is missing.

**Note:** each test-and-sex combination has one and only one
reference entry, so the unit and both limits are present on every
record, including one whose result is missing.

**Standard:** SDTM | **Domain:** LB
