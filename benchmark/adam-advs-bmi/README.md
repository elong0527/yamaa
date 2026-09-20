# Derive BMI

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-advs-bmi.html) [![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmark/README.md#lifecycle)

**Lifecycle:** reviewed - has discussion comments or GitHub issues.

**Goal:** add one BMI (body mass index) record for each collected
weight record, holding `BMI` as its code, `Body Mass Index
(kg/m^2)` as its name, the computed index in `AVAL`, and
`CALCULATION` in `DTYPE`.

**Input:** SDTM Vital Signs (VS) height and weight measurements across visits,
plus the subject's baseline height from the ADaM Subject-Level Analysis Dataset
(ADSL).

**Variables:**

- `AVAL` carries each collected measurement through. On a derived
  record it is the weight in kilograms divided by the square of
  the once-measured height in meters (the squared centimeter
  height divided by 10000); it is missing when that height is
  missing or zero. A weight record with a missing value yields no
  derived record.
- `DTYPE` is `CALCULATION` on a derived BMI record and
  blank on a collected record.

**Standard:** ADaM | **Domain:** ADVS
