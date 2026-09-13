# Apply an approved severity correction in ADAE

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-adae-severity-override.html)

**Goal:** derive `ASEV` (analysis severity) and `ASEVN` (its numeric
rank) for each adverse event (AE) in the Analysis Data Model (ADaM)
adverse event analysis dataset (ADAE), applying an approved severity
correction.

**Input:** collected AE records carrying the study, subject, and
event sequence identifiers and the reported severity `AESEV`.

**Variables:**

- `ASEV`: analysis severity in upper case (`MILD`, `MODERATE`,
  `SEVERE`, `LIFE-THREATENING`); the approved correction reassigns
  the one event covered by the approved correction in this example
  to `SEVERE`. An event with no reported severity and no applicable
  correction leaves `ASEV` empty.
- `ASEVN`: numeric rank of `ASEV`, from `1` (`MILD`) to `4`
  (`LIFE-THREATENING`). An event with no `ASEV` value has no rank.

**Note:** `ASEVN` reflects the corrected `ASEV`, so the corrected
event carries both the corrected term and its matching rank.

**Standard:** ADaM | **Domain:** ADAE
