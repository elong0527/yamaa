# Compute body mass index from height and weight

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adsl-bmi-compute.html) [![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmark/README.md#lifecycle)

**Goal:** compute body mass index (`BMI`) from collected height
and weight.

**Input:** subject-level records carrying `HEIGHTCM` in centimetres
and `WEIGHTKG` in kilograms.

**Variables:**

- `BMI` holds body mass index, `WEIGHTKG` divided by the square of
height in metres (`HEIGHTCM` divided by 100); it is missing when
`HEIGHTCM` is missing or equals zero, or when `WEIGHTKG` is missing.

**Standard:** ADaM | **Domain:** ADSL
