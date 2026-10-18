# Reject an Epoch the Study Never Declared

**Goal:** carry each vital-signs record's trial period (`EPOCH`), coded
with the study's epoch terminology.

**Input:** collected vital-signs records with the study and subject
identifiers, a sequence number and the recorded epoch.

**Variables:**

- `EPOCH` would be the trial period of the record, one of the epochs the
  terminology lists: `SCREENING`, `TREATMENT` or `FOLLOW-UP`; blank when
  no epoch was recorded.

**Note:** the published epoch list admits sponsor additions, but an
addition counts only once the study declares it. A record carrying
`RUN-IN`, an epoch the study's terminology never lists, rejects the run
once the epochs are complete, so no artifact is accepted. A blank epoch
passes.

## How to fix

Decide whether `RUN-IN` is a real trial period of this study. When it
is, add it to the study's epoch list. `RUN-IN` is a published epoch term,
so it is an ordinary item:

```yaml
- id: EPOCH
  name: Epoch
  extensible: true
  items:
    - {value: SCREENING}
    - {value: RUN-IN}
    - {value: TREATMENT}
    - {value: FOLLOW-UP}
```

Only a period the published list lacks is a sponsor addition, declared as
an item with `extended: true` and no NCI code, which the data-definition
document shows as an extended value. When `RUN-IN` is not a period of this
study, correct the collected epoch at its source and rerun.
