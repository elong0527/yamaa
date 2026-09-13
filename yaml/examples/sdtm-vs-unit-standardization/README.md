# Standardize collected results into standard units

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/sdtm-vs-unit-standardization.html)

**Goal:** build one record per collected vital-signs measurement,
carrying the collected test, result, and unit through and adding
the standardized numeric result, text result, and standard unit:
`VSTESTCD`, `VSTEST`, `VSORRES`, `VSORRESU`, `VSSTRESN`,
`VSSTRESC`, and `VSSTRESU`.

**Input:** collected vital-signs rows with test code and name,
collected result and unit, plus study, subject, sequence, and
visit identifiers.

**Variables:**

- `VSTESTCD` is the test code as collected: `HEIGHT`, `WEIGHT`,
  or `TEMP`.
- `VSTEST` is the test name as collected: `Height`, `Weight`, or
  `Temperature`.
- `VSORRES` is the result exactly as collected, never overwritten.
- `VSORRESU` is the unit exactly as collected: `cm`, `kg`, `C`,
  `LB`, or `F`.
- `VSSTRESN` is the numeric result in the test's standard unit:
  height passes through in `cm`; weight in `LB` is multiplied by
  `0.45359237` to reach `kg`; temperature in `F` becomes
  (`VSORRES` - 32) * 5 / 9 to reach `C`; a result already in the
  standard unit passes through unchanged; a result never collected
  stays missing.
- `VSSTRESC` is the same standardized value written as text, so it
  always agrees with the numeric result; when the collected unit
  is already the standard unit, it equals the collected result.
  Blank when no result was collected.
- `VSSTRESU` is the test's standard unit: `cm` for height, `kg`
  for weight, and `C` for temperature. Blank when no result was
  collected.

**Note:** records are grouped by test rather than kept in
collection order, so each test stands alone; adding a test means
describing that test, not changing the others. A record whose
result was never collected carries no standardized value in any of
the three added columns.

**Standard:** SDTM | **Domain:** VS
