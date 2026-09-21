# Parquet Primary Output

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/schema-parquet-output.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** publish the primary output as a Parquet file. The
output path carries a `.parquet` extension, so the artifact
leaves through the Parquet profile: typed fields, missing
values as nulls, and numbers stored rather than rendered.

**Input:** one flat subject file, `input/subjects.csv`, with
sex, age, height, weight, and treatment start date per
subject.

**Variables:**

- `SEX`: recorded sex; null when uncollected.
- `AGE`: age in whole years as collected; null when
  uncollected.
- `BMI`: body mass index from height and weight; null when
  either measure is uncollected.
- `TRTSDT`: date treatment started; null when uncollected.

**Note:** a missing value is a null in the Parquet file, not
an empty field, and `BMI` keeps the full computed value
with no display rounding.

**Standard:** SDTM | **Domain:** DM
