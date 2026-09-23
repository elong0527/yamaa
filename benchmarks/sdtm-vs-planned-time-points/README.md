# Planned Time Points Around Dosing

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-vs-planned-time-points.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** carry each collected vital-signs result with its actual
collection datetime, and add the planned time point it was
scheduled for: `VSTESTCD`, `VSORRES`, `VSTPT`, `VSTPTNUM`,
`VSELTM`, `VSDTC`, and `VSDY`.

**Input:** long-form Operational Data Model (ODM) data with one row
per collected item, carrying the collection form (`FormOID`), the
item (`ItemOID`), and the stored value (`Value`); plus
demographics rows carrying the reference start date. Each planned
time point arrives as its own vital-signs form, so the form tells
which scheduled reading a result belongs to. The first-dose
datetime rides along on the dosing form `FO.EX`.

**Variables:**

- `VSTESTCD` is the test short name, one row template per test:
  `PULSE`, `SYSBP`, or `DIABP`.
- `VSORRES` is the result in original units, the value on the
  test's item row within the form.
- `VSORRESU` is the original unit from the matching unit item
  row: `beats/min` or `mmHg`.
- `VSTPT` is the planned time point name for the collection
  form: `PRE-DOSE`, `30 MIN POST-DOSE`, `1 H POST-DOSE`, or
  `4 H POST-DOSE`.
- `VSTPTNUM` is the planned time point number for the form:
  1 through 4 in schedule order.
- `VSELTM` is the planned elapsed time since the first dose for
  the form, in ISO 8601 duration form: `-PT15M` before the dose,
  then `PT30M`, `PT1H`, and `PT4H` after it.
- `VSDTC` is the actual collection datetime from the form's
  datetime item row. A reading taken later than its planned
  elapsed time keeps its late actual datetime, so the gap shows
  when the two are compared.
- `VSSTRESN` is the numeric result in standard units; pulse and
  blood pressure were collected in standard units, so it equals
  the original result.
- `VSSTRESU` is the standard unit, carried from the original
  unit.
- `VSSEQ` numbers the records per subject in schedule order,
  pulse then blood pressure within each time point.
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
