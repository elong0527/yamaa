# Reject Declared Types on ODM Data

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-odm-schema-type.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** derive `AGE` for one record per subject from the subject's
collected answer.

**Input:** long-form Operational Data Model (ODM) data with one row per
collected item, declared with the visit repeat read as a number.

**Variables:**

- `AGE` would be the subject's collected age.

ODM data has one fixed layout, every field text, and the declaration would
change it, so the input is rejected before any record is read.

**Standard:** SDTM | **Domain:** DM

## How to fix

Declare the ODM data by its path alone. A value needed as a number is typed
where it is used: `AGE` is declared `int`, and the collected text converts
to it. A visit repeat used as a number is read into its own column:

```yaml
- name: VISITREP
  type: int
  derivation: ODM.StudyEventRepeatKey
```
