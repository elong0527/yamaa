# Record randomization timing

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adsl-randomization-timing.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** record randomization timing for each subject
(`RANDDT`, `RANDDY`, `RANDDTC`).

**Input:** demographics (DM) records carrying the reference
start date (`RFSTDTC`), the randomization date (`RANDDT`), and
the randomization moment (`RANDDTTM`).

**Variables:**

- `RANDDT` is the date of randomization as collected. It is
  missing when no randomization date was collected.
- `RANDDY` is the study day of randomization measured from
  `RFSTDTC`: the reference date itself is day `1`, later dates
  count forward inclusively, and earlier dates count backward
  with no day `0`, so the day before the reference date is day
  `-1`. It is missing when either date is absent.
- `RANDDTC` is the randomization moment (`RANDDTTM`) written
  as text at whole-second precision. It is missing when no
  randomization moment was collected.

**Standard:** ADaM | **Domain:** ADSL
