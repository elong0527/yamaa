# Reject a row with only some paired dates present

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-all-or-none-partial-row.html)

**Goal:** carry the treatment start date `RFSTD` and the death
date `DTHDT` into a subject-level record, where the two dates must
be present together or absent together.

**Input:** collected demographics records holding the study and
subject identifiers with the treatment start date in `RFSTDT` and
the date of death in `DTHDTC`, which is missing when the subject
has not died.

**Variables:**

- `RFSTD` would be the date treatment started, taken from
  `RFSTDT` in the demographics input, but this run is rejected so
  no dataset is accepted.
- `DTHDT` would be the date of death, taken from `DTHDTC` in the
  demographics input and missing when the subject has not died,
  but this run is rejected so no dataset is accepted.

A record with only one of the two dates present fails the
pairing, so the run is rejected and no artifact is accepted.

**Note:** a record with both dates present and a record with both
dates missing both satisfy the pairing; only a half-present pair
fails.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

Decide which absences the study allows, then assert each side on its
own. When every subject must have a start date but only some have died,
require the start and leave death optional:

```yaml
columns:
  - name: RFSTD
    type: date
    verifications:
      - not_missing
```

When two dates truly travel together, keep the pairing and correct the
offending row instead of loosening the rule.
