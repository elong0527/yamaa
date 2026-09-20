# Derive BMI

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-advs-bmi.html) [![Lifecycle: finalized](https://img.shields.io/badge/Lifecycle-finalized-brightgreen)](https://github.com/elong0527/yamaa/blob/main/benchmark/README.md#lifecycle)

**Goal:** derive BMI record for each collected weight record
in ADVS using baseline height.

**Input:** A VS dataset with height and weight information and an ADSL
dataset with baseline height information.

**Variables:**

- `AVAL` is the collected measurement on collected records and
  the BMI on BMI records; missing without a usable height, and
  no BMI record is made without a weight.

**Standard:** ADaM | **Domain:** ADVS
