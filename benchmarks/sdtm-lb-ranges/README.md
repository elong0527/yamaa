# Reference Range Flags

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-lb-ranges.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

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

**Note:** a test-and-sex combination with two reference entries
stops the run. One with no entry leaves the unit and both limits
blank, and its result is flagged `NORMAL`. A record whose result is
missing still carries its unit and limits.

**Standard:** SDTM | **Domain:** LB
