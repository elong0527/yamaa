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
is, declare it as a sponsor addition to the extensible list, so the
data-definition document also shows it as an extended value:

```yaml
- id: EPOCH
  name: Epoch
  extensible: true
  items:
    - {value: SCREENING}
    - {value: RUN-IN, extended: true}
    - {value: TREATMENT}
    - {value: FOLLOW-UP}
```

When it is not, correct the collected epoch at its source and rerun.
