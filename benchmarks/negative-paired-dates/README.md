# Reject Dates That Are Not Paired

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-paired-dates.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** carry the treatment start date `RFSTD` and the death
date `DTHDT` into a subject-level record, where the two dates must
be present together or absent together.

**Input:** collected demographics records, one per subject, holding
the treatment start date in `RFSTDT` and the date of death in
`DTHDTC`.

**Variables:**

- `RFSTD` would be the date treatment started, taken from
  `RFSTDT` in the demographics input.
- `DTHDT` would be the date of death, taken from `DTHDTC` in the
  demographics input and missing when the subject has not died.

**Note:** a record with both dates present or both missing passes;
a record with only one present fails the pairing, so the whole run
is rejected and no dataset is accepted.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

Decide which absences the study allows, then assert each side on its
own. When every subject must have a start date but only some have died,
remove the pairing, require the start and leave death optional:

```yaml
columns:
  - name: RFSTD
    type: date
    label: Reference Start Date
    derivation: DM.RFSTDT
    verifications:
      - not_missing: {}
```

When two dates truly travel together, keep the pairing and correct the
offending row instead of loosening the rule.
