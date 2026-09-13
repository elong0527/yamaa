# Derive body mass index at each weight visit

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-advs-body-mass-index.html)

**Goal:** add one body mass index (BMI) record for each collected
weight record, holding `BMI` as its code, `Body Mass Index
(kg/m^2)` as its name, the computed index in `AVAL`, and
`CALCULATION` in `DTYPE`.

**Input:** vital signs with the subject's height record (`HEIGHT`
named `Height (cm)`) and weight records across visits (`WEIGHT`
named `Weight (kg)`), each carrying its measured value and visit.

**Variables:**

- `AVAL` carries each collected measurement through. On a derived
  record it is the weight in kilograms divided by the square of
  the once-measured height in meters (the squared centimeter
  height divided by 10000); it is missing when that height is
  missing or zero. A weight record with a missing value yields no
  derived record.
- `DTYPE` is `CALCULATION` on a derived body mass index record and
  blank on a collected record.

**Standard:** ADaM | **Domain:** ADVS
