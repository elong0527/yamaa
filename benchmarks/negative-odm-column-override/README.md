# Reject a Template Overriding an Item Read

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-odm-column-override.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** derive `SEX` for each subject from the subject's collected
answer, while the template that builds the subjects also sets `SEX`.

**Input:** long-form Operational Data Model (ODM) data with one row per
collected item.

**Variables:**

- `SEX` is read from the collected answer by the column, and set to `U`
  by the row template.

An item read takes the collected records of the row it answers for, so it
is not a per-row default a template may replace. The column and the
template would both derive `SEX`, and the request is rejected before any
data is read.

**Standard:** SDTM | **Domain:** DM

## How to fix

Derive `SEX` in one place. Keep the item read on the column and drop it
from the template:

```yaml
columns:
  - name: SEX
    derivation: {odm: ODM.IT.DM.SEX}
rows:
  - id: subjects
    dataset: ODM
    group_by: [ODM.StudyOID, ODM.SubjectKey]
    derivations:
      STUDYID: ODM.StudyOID
      USUBJID: ODM.SubjectKey
```
