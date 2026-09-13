# Reconcile randomization strata

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-adam-adsl-stratification-reconciliation.html)

**Goal:** build `STRAT1R` from the values recorded at
randomization and `STRAT1` from the matching values collected
independently at entry, and require the two combined values to
agree.

**Input:** demographics entries, disease-history entries, and the
strata recorded at randomization, linked by the study and subject
identifiers shared across the three files.

**Variables:**

The study and subject identifiers carry through unchanged.

- `METSTATR`: metastatic disease status recorded at
  randomization, copied from input `METSTAT` in the
  randomization file.
- `ECOG0R`: baseline performance status on the Eastern
  Cooperative Oncology Group (ECOG) scale recorded at
  randomization, copied from input `ECOG0` in the
  randomization file.
- `REGIONUSR`: region recorded at randomization, copied from
  input `REGIONUS` in the randomization file.
- `METSTAT`: metastatic disease status from the disease-history
  entries, copied from input `METSTAT` there.
- `ECOG0`: baseline ECOG performance status from the
  demographics entries, copied from input `ECOG0` there.
- `REGIONUS`: region from the demographics entries, copied from
  input `REGIONUS` there.
- `STRAT1R`: combined randomization strata, joining `METSTATR`,
  `ECOG0R`, and `REGIONUSR` with `|`.
- `STRAT1`: combined independently collected strata, joining
  `METSTAT`, `ECOG0`, and `REGIONUS` with `|` in the same order.

The two combined values must agree. When the two values disagree
for any subject, the run is rejected and no artifact is accepted
from this input.

**Note:** the expected file records the completed rows as
presented to the check; the disagreement still rejects the run.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

Query the disagreement and correct whichever source is wrong. If both values
are valid but serve different purposes, document which one governs the analysis
and replace the equality check with that reconciliation policy. Do not silently
prefer the randomization value or the independently collected value.
