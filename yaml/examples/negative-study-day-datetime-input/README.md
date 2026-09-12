# ADaM ADSL: reject a study day measured from a moment

This example uses collected demographics to record one row per subject:

- `RFSTD` is the calendar date treatment started.
- `DTHDTM` is the moment of death, time of day included.
- `DTHDY` is the study day of death, counting whole days from day one.

A study day counts dates, and a moment is not one: widening it would
choose silently between two adjacent days, so no implementation may do
so. The specification is rejected before any data is read and no
artifact is accepted.

## How to fix

Decide which calendar day the study reports, then state it as a date.
When only the moment is collected, take its calendar date first so the
day count has whole days to count:

```yaml
- name: DTHDT
  type: date
  derivation:
    to_date:
      source: DTHDTM
```

When the time of day carries meaning, a day count is the wrong result;
keep the moment instead of forcing it into one.
