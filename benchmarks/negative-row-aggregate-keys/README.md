# Reject a subject-wide count inside a one-race row

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-row-aggregate-keys.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** attempt one supplemental Demographics (SUPPDM) record per
reported race (`QVAL`) for each subject who marked more than one race,
deciding "more than one" by counting the subject's race rows.

**Input:** EDC output in long form, one row per collected item; each race
option a subject marks contributes one race row.

**Variables:**

- **QVAL**: would be one race the subject marked, kept only for a subject
  with several; no value is produced.

Each record being built stands for one subject and one race, so a count
taken while building it sees only that race's rows. Asking the count to
span the whole subject does not widen it, and the run would keep no
record at all without saying why, so it is rejected before any data is
read.

**Standard:** SDTM | **Domain:** SUPPDM

## How to fix

Decide which record already says that a subject marked several races, and
read it instead of recounting inside the record being built. When the
same run builds DM, its `RACE` is `MULTIPLE` exactly then: read DM through
an input with `schema:` and filter on `DMRACE = 'MULTIPLE'`.

Without DM, select the subject's lowest and highest race with keyed
lookups and keep the record when they differ. Unlike a row count, this
does not mistake a race marked twice for two races:

```yaml
intermediates:
  - id: RACELOW
    dataset: ODM
    key: [StudyOID, SubjectKey]
    key_base: [STUDYID, USUBJID]
    filter: "ODM.ItemOID = 'IT.DM.RACE' AND ODM.Value IS NOT NULL"
    order_by: [ODM.Value]
    keep: first
  # RACEHIGH: the same lookup with keep: last
rows:
  - id: reported_race
    group_by: [ODM.StudyOID, ODM.SubjectKey, ODM.ItemOID, ODM.Value]
    filter: "ITEMOID = 'IT.DM.RACE' AND RACELOWV < RACEHIGHV"
    derivations:
      # STUDYID, USUBJID, ITEMOID and QVAL as before
      RACELOWV: RACELOW.Value
      RACEHIGHV: RACEHIGH.Value
```
