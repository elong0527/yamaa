# Derive BMI

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-advs-bmi.html)
[![Lifecycle: finalized](https://img.shields.io/badge/Lifecycle-finalized-brightgreen)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** derive one body mass index (BMI) record for each
collected weight record with a result, holding the index in `AVAL`.

**Input:** collected vital signs with height and weight records
per subject and visit, plus each subject's baseline height.

**Variables:**

- `PARAMCD` keeps each collected test code (`HEIGHT`, `WEIGHT`)
  and marks each new record `BMI`.
- `PARAM` keeps each collected test name and labels each new
  record `Body Mass Index (kg/m^2)`.
- `AVAL` keeps each collected result. On a BMI record it is the
  collected weight in kilograms divided by the square of the
  baseline height in metres. It is empty when the baseline height
  is missing or zero. A weight record without a result gets no BMI
  record.

**Note:** a missing or zero baseline height leaves the BMI record
in place with an empty value instead of dropping it; a weight
record with no result is the only case that produces no BMI
record.

**Standard:** ADaM | **Domain:** ADVS
