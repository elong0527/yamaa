# One Template Replacing a Column Default

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/schema-rows-override.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** show that a single row template can replace a column's
stated default for its own rows while every other template keeps it:
weight records gain a body-mass-index record computed from baseline
height, and all other records carry the standardized result unchanged.

**Input:** one `spec.yaml` with two row templates over a vital-signs
extract (`input/vs.csv`: height, weight, and temperature records with
standardized numeric results) and a subject-level file
(`input/adsl.csv`) carrying each subject's baseline height in
centimetres.

**Variables:**

- `AVAL`: the analysis value. For body-mass-index rows, weight in
  kilograms divided by squared baseline height in metres; for every
  other row, the standardized result exactly as collected. A
  body-mass-index row with no usable baseline height keeps `AVAL`
  blank, as does a row whose collected result is blank.
- `PARAMCD`: which record a row carries: the collected test code
  (`HEIGHT`, `WEIGHT`, `TEMP`), or `BMI` for the derived
  body-mass-index rows.
- `PARAM`: the matching test name, or `Body Mass Index (kg/m^2)` for
  the derived rows.

**Note:** a column's stated default applies to every row template. A
template that restates the column replaces the default for its own
rows only, and the replacement reads that template's names exactly as
if it had been written inside the template. The collected template
therefore carries weight records through unchanged: the replacement
never leaks onto them, while the body-mass-index template divides
each weight by its own subject's baseline height.

**Standard:** ADaM | **Domain:** ADVS
