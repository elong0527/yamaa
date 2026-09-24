# Derive BMI Two Ways

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adsl-bmi.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** derive body mass index (`BMI`) two ways - once as an inline
expression and once via a routine the project supplies (`BMI_FN`) -
and show both columns carry the same value on every record.

**Input:** subject-level records carrying height in centimetres
(`HEIGHTCM`) and weight in kilograms (`WEIGHTKG`); both are carried
through unchanged.

**Variables:**

- `BMI` holds body mass index: `WEIGHTKG` divided by the square of
  height in metres (`HEIGHTCM` divided by 100). It is empty when
  `HEIGHTCM` is missing or zero, or when `WEIGHTKG` is missing.
- `BMI_FN` carries the same index as calculated by the project
  routine from `HEIGHTCM` and `WEIGHTKG`; empty when height or
  weight is missing.

**Note:** both columns agree on every record: a zero weight gives a
zero index, and a missing height or weight leaves both empty.

**Standard:** ADaM | **Domain:** ADSL
