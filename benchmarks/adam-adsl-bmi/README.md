# Derive BMI

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adsl-bmi.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** derive body mass index two ways - once as an inline
expression and once via a routine the project supplies - and show
both give the same answer.

**Input:** subject-level records carrying `HEIGHTCM` in centimetres
and `WEIGHTKG` in kilograms.

**Variables:**

- `BMI` holds body mass index, `WEIGHTKG` divided by the square of
  height in metres (`HEIGHTCM` divided by 100); it is missing when
  `HEIGHTCM` is missing or equals zero, or when `WEIGHTKG` is missing.
- `BMI_FN` carries the same index as calculated by the project
  routine (`bmi`) from `HEIGHTCM` and `WEIGHTKG`; empty when height
  or weight is missing.

**Standard:** ADaM | **Domain:** ADSL
