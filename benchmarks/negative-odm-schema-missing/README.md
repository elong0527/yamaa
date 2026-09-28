# Reject an Extract Without Its Form Level

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-odm-schema-missing.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** derive `AGE` for one record per subject from the subject's
collected answer.

**Input:** long-form Operational Data Model (ODM) data with one row per
collected item. The export leaves out the form and form repeat fields that
every ODM export carries.

**Variables:**

- `AGE` would be the subject's collected age.

Without the form level a record's place in the collection cannot be told,
so the input is rejected before any record is read, naming the two fields
it lacks.

**Standard:** SDTM | **Domain:** DM

## How to fix

Export the ODM data with every level of its hierarchy, the form and its
repeat included. When the source nests item groups without a form, the
outer group is the form, which is how the engine's own ODM reader writes
it:

```text
StudyOID,MetaDataVersionOID,SubjectKey,StudyEventOID,StudyEventRepeatKey,
FormOID,FormRepeatKey,ItemGroupOID,ItemGroupRepeatKey,ItemOID,Value
```
