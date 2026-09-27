# Standardize Vital-Signs Units

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-vs-units.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** turn every collected vital-signs measurement into one
record that keeps the collected test, result, and unit, and adds
the result in the test's standard unit (as a number and as text)
plus a completion status: `VSTESTCD`, `VSTEST`, `VSORRES`,
`VSORRESU`, `VSSTRESN`, `VSSTRESC`, `VSSTRESU`, and `VSSTAT`.

**Input:** collected vital-signs rows, each with a test code and
name, the result as collected with its unit, and study, subject,
sequence, and visit identifiers.

**Variables:**

- `VSTESTCD` is the test code as collected: `HEIGHT`, `WEIGHT`,
  or `TEMP`.
- `VSTEST` is the test name as collected: `Height`, `Weight`, or
  `Temperature`.
- `VSORRES` is the collected result written back as text; reading
  it as a number drops trailing zeros (a collected `72.50` reads
  `72.5`), but the value itself is never converted.
- `VSORRESU` is the unit exactly as collected: `cm`, `kg`, `C`,
  `LB`, or `F`. Blank when no result was collected.
- `VSSTRESN` is the result as a number in the test's standard
  unit: height passes through in `cm`; weight in `LB` is
  multiplied by `0.45359237` to reach `kg`; temperature in `F`
  becomes (`VSORRES` - 32) * 5 / 9 to reach `C`; a result
  already in the standard unit passes through unchanged; a result
  never collected stays missing.
- `VSSTRESC` is the same standardized value written as text, so
  it always agrees with the numeric result. Blank when no result
  was collected.
- `VSSTRESU` is the test's standard unit: `cm` for height, `kg`
  for weight, `C` for temperature. A unit that does not belong to
  the test (a weight in `cm`) stops the run instead of being
  carried through as the standard unit. Blank when no result was
  collected.
- `VSSTAT` is `NOT DONE` when no result was collected, blank
  otherwise.

**Note:** records are grouped by test rather than kept in
collection order: all heights first, then all weights, then all
temperatures, each in collection order. A record whose result was
never collected carries `VSSTAT` `NOT DONE`, no original unit,
and no standardized value.

**Standard:** SDTM | **Domain:** VS
