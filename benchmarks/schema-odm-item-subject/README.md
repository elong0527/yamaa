# One Collected Answer per Subject

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/schema-odm-item-subject.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** derive `SEX` and `AGE` for one record per subject, each from the
one record that holds the subject's answer.

**Input:** long-form Operational Data Model (ODM) data with one row per
collected item, carrying the visit (`StudyEventOID`), the form, the item
(`ItemOID`), and the stored value (`Value`).

**Variables:**

- `SEX` is the sex answered at screening. A subject who is asked again at
  a later visit keeps the screening answer.
- `AGE` is the age collected for the subject; a subject with no age record
  keeps a missing age.

**Note:** an answer is read only when exactly one record holds it, so two
records of one question at one visit stop the run even when they agree.
Items the record does not read, such as a weight, play no part.

**Standard:** SDTM | **Domain:** DM
