# Body mass index from a project routine

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-adsl-bmi-function.html)

**Goal:** add body mass index (BMI) for each subject, carried
alongside the collected height (`HEIGHTCM`) and weight
(`WEIGHTKG`), with the index computed by a routine the project
supplies.

**Input:** collected subject measurements carrying height in
centimetres (cm) (`HEIGHTCM`) and weight in kilograms (kg)
(`WEIGHTKG`).

**Variables:**

- `BMI`: body mass index in kg/m2, as calculated by the project
  routine from `HEIGHTCM` and `WEIGHTKG`; empty when height or
  weight is missing.

**Standard:** ADaM | **Domain:** ADSL
