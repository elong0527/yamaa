# Collected Vital Signs Form

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-vs-collected-form.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** build one record per measurement from the wide vital
signs form collected at each visit, carrying `VSTESTCD`,
`VSTEST`, `VSORRES`, `VSORRESU`, `VSSTRESN`, `VSSTRESC`,
`VSSTRESU`, `VSPOS`, `VSMETHOD`, `VSSTAT`, and `VSREASND`, plus
the visit name in `VISIT` and the measurement date in `VSDTC`.

**Input:** one form row per visit with a field per measurement
(systolic and diastolic blood pressure, pulse, respiratory rate,
temperature, weight), each with its unit, plus body position,
temperature method, and a blood pressure not-done flag with its
reason.

**Variables:**

- `VSTESTCD` is the test code: `SYSBP`, `DIABP`, `PULSE`,
  `RESP`, `TEMP`, or `WEIGHT`.
- `VSTEST` is the test name: `Systolic Blood Pressure`,
  `Diastolic Blood Pressure`, `Pulse Rate`, `Respiratory Rate`,
  `Temperature`, or `Weight`.
- `VSORRES` is the result exactly as collected on the form;
  blank when the measurement was not done.
- `VSORRESU` is the unit exactly as collected: `mmHg`,
  `beats/min`, `breaths/min`, `C`, or `kg`; blank when no
  result was collected.
- `VSSTRESN` is the numeric result in the standard unit; the
  form already collects standard units, so it equals the
  collected result; missing when no result was collected.
- `VSSTRESC` is the same standardized value written as text,
  so it always agrees with the numeric result; blank when no
  result was collected.
- `VSSTRESU` is the standard unit, equal to the collected unit;
  blank when no result was collected.
- `VSPOS` is the body position as collected on the form
  (`SITTING`) on blood pressure and pulse records with a
  collected result; blank otherwise.
- `VSMETHOD` is the measurement method as collected on the
  form (`ORAL`) on temperature records with a collected
  result; blank otherwise.
- `VSSTAT` is `NOT DONE` when the form flags blood pressure as
  not done; blank otherwise.
- `VSREASND` is the reason the form gives for blood pressure
  not done; blank otherwise.

**Note:** each form row fans out into one record per test, and
records are grouped by test rather than kept in form order;
`VSSEQ` numbers them per subject by visit date, then in form
order. A not-done blood pressure still produces its two records
so the reason is kept, but they carry no result, unit, position,
or standardized value.

**Standard:** SDTM | **Domain:** VS
