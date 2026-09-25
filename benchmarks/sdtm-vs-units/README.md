# Unit Standardization

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-vs-units.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

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
- `VSORRES` is the collected result, read as a number and written
  back as text (so a collected `72.50` reads `72.5`); the conversion
  never overwrites it.
- `VSORRESU` is the unit exactly as collected: `cm`, `kg`, `C`,
  `LB`, or `F`. Blank when no result was collected.
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
  for weight, and `C` for temperature. An unexpected unit for the
  test (for example a weight in `cm`) fails the run instead of
  passing through. Blank when no result was collected.
- `VSSTAT` is `NOT DONE` when no result was collected, blank
  otherwise.

**Note:** records are grouped by test rather than kept in
collection order, so each test stands alone; adding a test means
describing that test, not changing the others. A record whose
result was never collected carries `VSSTAT` `NOT DONE`, no original
unit, and no standardized value in any of the three added columns.

**Standard:** SDTM | **Domain:** VS
