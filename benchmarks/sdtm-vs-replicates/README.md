# Replicate Readings

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-vs-replicates.html) [![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** keep every blood pressure reading taken at a visit and add
the protocol's visit value beside them: one record per reading with
its replicate number, plus one record per test and visit holding the
mean of the readings actually taken, marked as a derived record.

**Input:** raw vital-signs rows with test code and name, result and
unit, visit, and the replicate number of each reading.

**Variables:**

- `VSTESTCD` is the test code as collected: `SYSBP` or `DIABP`.
- `VSTEST` is the test name as collected.
- `VSREPNUM` numbers the readings of one test at one visit; it stays
  blank on the derived mean record.
- `VSORRES` is the result exactly as collected on a reading record;
  on the mean record it carries the mean.
- `VSORRESU` is the unit as collected: `mmHg`.
- `VSSTRESN` is the numeric result: the reading itself on a reading
  record, the mean of the readings actually taken on the mean record.
- `VSSTRESC` is the same value written as text, so it always agrees
  with the numeric result.
- `VSDRVFL` marks the mean record with `Y` and stays blank on the
  reading records.
- `VSSEQ` numbers every record of a subject in one sequence:
  readings first in replicate order, then the mean record, for each
  test and visit.

**Note:** one subject's second visit has only two systolic readings;
its mean is taken over the two readings present.

**Standard:** SDTM | **Domain:** VS
