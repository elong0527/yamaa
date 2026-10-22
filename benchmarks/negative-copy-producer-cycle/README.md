# Reject Two Datasets That Each Take a Value From the Other

**Goal:** carry each subject's age (`AGE`) and first dose date (`TRTSDT`)
onto the subject-level analysis dataset.

**Input:** collected demographics with the age, and the saved exposure
summary with the first dose date. `spec_exsum.yaml` produces that summary,
and it takes each subject's age from the subject-level dataset in turn.

**Variables:**

- `AGE` would be the age in years as collected.
- `TRTSDT` would be the first dose date as the exposure summary states it.

**Note:** the subject-level dataset takes its first dose date from the
exposure summary, and the exposure summary takes its age from the
subject-level dataset. Each would have to be built before the other, so no
order can build them, and the run is rejected before any data is read; no
artifact is accepted.

## How to fix

Decide which dataset owns each value, and take it from its own source rather
than from the other dataset. Here the exposure summary can read the age from
the collected demographics instead of the subject-level dataset:

```yaml
input:
  EX: {path: input/ex_first.csv, types: {TRTSDT: date}}
  DM: {path: input/dm.csv, key: [STUDYID, USUBJID]}
```

with the age read as `DM.AGE`, so that only the subject-level dataset reads
the exposure summary.
