# Replicate Readings

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-vs-replicates.html) [![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** keep every blood pressure reading taken at a visit and add
the protocol's visit value beside them: one record per reading with
its replicate number, plus one record per test and visit holding the
mean of the readings actually taken, marked as a derived record.

**Input:** long-form Operational Data Model (ODM) rows, one row per
collected item, with the collected entry in the value field. Each
replicate of a blood pressure reading is one repeat of the
vital-signs item group, numbered in the item-group repeat key; the
visit rides in the study event.

**Variables:**

- `VSTESTCD` is the test code of the collected item: `SYSBP` or
  `DIABP`.
- `VSTEST` is the test name of the collected item.
- `VSREPNUM` is the item-group repeat key: it numbers the readings
  of one test at one visit, and stays blank on the derived mean
  record.
- `VSORRES` is the reading written as text from its numeric value, so
  trailing zeros are dropped (a collected `82.50` reads `82.5`); on the
  mean record it carries the mean.
- `VSORRESU` is `mmHg` for every record.
- `VSSTRESN` is the numeric result: the reading itself on a reading
  record, the mean of the readings actually taken on the mean record.
- `VSSTRESC` is the same value written as text, so it always agrees
  with the numeric result.
- `VSDRVFL` marks the mean record with `Y` and stays blank on the
  reading records.
- `VSSEQ` numbers every record of a subject in one sequence, by
  visit name, then test code (`DIABP` before `SYSBP`): each test's
  readings in replicate order, then its mean record.

**Note:** a reading that was not taken, whether its record is absent
or its value is blank, gives no reading record, and the visit mean is
taken over the readings present.

**Standard:** SDTM | **Domain:** VS
