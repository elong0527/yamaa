# Accumulation Ratios Between Day 1 and Steady State

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adpp-accumulation-ratio.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** keep every pharmacokinetic (PK) parameter record of a
multiple-dose study and add, per subject, the accumulation ratios of the
area under the curve over the dosing interval (`ARAUC`) and of the maximum
concentration (`ARCMAX`): the steady-state value divided by the day 1 value.

**Input:** the SDTM PK parameters (PP), one record per subject, parameter
and profile visit, with the standard numeric result (`PPSTRESN`) and unit
(`PPSTRESU`); and the subject-level dataset (ADSL) with the planned arm
(`ARMCD`, `ARM`). Arm `A` is dosed once daily on days 1 to 7, arm `B` on
days 1 to 5.

**Variables:**

- `PARAMCD`, `PARAM`: the collected `PPTESTCD` and `PPTEST`; on the new
  records `ARAUC` "Accumulation Ratio AUCTAU" and `ARCMAX`
  "Accumulation Ratio Cmax", the controlled terms.
- `AVISIT`: the collected visit (`DAY 1`, `DAY 5`, `DAY 7`); on a ratio
  record the visit of the steady-state profile.
- `AVAL`: the collected standard result, empty when the parameter was not
  calculated; on a ratio record the steady-state `AUCTAU` or `CMAX` divided
  by the day 1 value of the same subject, not rounded and possibly below 1.
- `AVALU`: the collected standard unit; `RATIO` on a ratio record.

**Note:** the steady-state profile is the one on the last planned dosing
day of the subject's arm (day 7 for arm `A`, day 5 for arm `B`), and the
first-dose profile is the day 1 profile. A profile on any other day, such as
an interim profile during dosing or the last profile of a subject who
stopped dosing early, enters neither value. A ratio record exists only when
both values are present; a parameter that was not calculated keeps its own
record with no value.

**Standard:** ADaM | **Domain:** ADPP
