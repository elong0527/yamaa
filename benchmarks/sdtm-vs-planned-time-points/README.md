# Planned Time Points Around Dosing

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-vs-planned-time-points.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** carry each collected vital-signs result with its actual
collection datetime, and add the planned time point it was
scheduled for: `VSTESTCD`, `VSORRES`, `VSTPT`, `VSTPTNUM`,
`VSELTM`, `VSDTC`, and `VSDY`.

**Input:** collected vital-signs rows with the planned time point
label, test, result, and actual collection datetime; a planned
time-point table carrying the number and the planned elapsed time
per label; exposure rows carrying the first-dose datetime; and
demographics rows carrying the reference start date.

**Variables:**

- `VSTESTCD` is the test short name, carried over unchanged:
  `PULSE`, `SYSBP`, or `DIABP`.
- `VSORRES` is the result in original units, carried over
  unchanged.
- `VSORRESU` is the original unit, carried over unchanged:
  `beats/min` or `mmHg`.
- `VSTPT` is the planned time point name, carried over unchanged.
- `VSTPTNUM` is the planned time point number from the
  time-point table for the collected label: 1 for `PRE-DOSE`,
  2 for `30 MIN POST-DOSE`, 3 for `1 H POST-DOSE`, and 4 for
  `4 H POST-DOSE`.
- `VSELTM` is the planned elapsed time since the first dose from
  the time-point table, in ISO 8601 duration form: `-PT15M`
  before the dose, then `PT30M`, `PT1H`, and `PT4H` after it.
- `VSDTC` is the actual collection datetime, carried over
  unchanged. A reading taken later than its planned elapsed time
  keeps its late actual datetime, so the gap shows when the two
  are compared.
- `VSSTRESN` is the numeric result in standard units; pulse and
  blood pressure were collected in standard units, so it equals
  the original result.
- `VSSTRESU` is the standard unit, carried from the original
  unit.
- `VSDY` is the study day of the collection datetime, counted
  from the subject's reference start date: that date is day 1,
  there is no day zero, and dates before it count back from -1;
  missing when the result has no datetime or the subject has no
  reference start date.

**Note:** the planned elapsed time stays in duration form next to
the actual collection datetime; the comparison between them is
where a late reading shows up. The first-dose datetime and the
reference start date are used and then dropped, since neither is
part of the result record.

**Standard:** SDTM | **Domain:** VS
