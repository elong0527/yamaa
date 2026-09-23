# Severity Correction

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adae-severity-override.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** derive `ASEV` (analysis severity) and `ASEVN` (its numeric
rank) for each adverse event (AE) in the Analysis Data Model (ADaM)
adverse event analysis dataset (ADAE), applying an approved severity
correction.

**Input:** collected AE records carrying the study, subject, and
event sequence identifiers and the reported severity `AESEV`.

**Variables:**

- `ASEV`: analysis severity in upper case (`MILD`, `MODERATE`,
  `SEVERE`, `LIFE-THREATENING`); an approved correction reassigns
  any event it covers to `SEVERE`. An event with no reported
  severity and no applicable correction leaves `ASEV` empty.
- `ASEVN`: numeric rank of `ASEV`, from `1` (`MILD`) to `4`
  (`LIFE-THREATENING`). An event with no `ASEV` value has no rank.

**Note:** `ASEVN` reflects the corrected `ASEV`, so a corrected
event carries both the corrected severity and its matching rank.

**Standard:** ADaM | **Domain:** ADAE
