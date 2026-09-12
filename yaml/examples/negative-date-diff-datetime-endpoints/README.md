# ADaM ADSL: reject counting days between two moments

This example uses collected demographics to record one row per subject:

- `RFSTDTM` is the moment treatment started, time of day included.
- `DTHDT` is the calendar date of death.
- `SURVDD` counts whole days between them.

A day count has no meaning between two moments: the hours on either
side could defend two different answers, so no implementation may widen
a moment into a date silently. The specification is rejected before any
data is read and no artifact is accepted.

## How to fix

Decide which calendar days the study counts, then state them as dates.
When only moments are collected, take each moment's calendar date first
so the count has whole days to count:

```yaml
- name: RFSTD
  type: date
  derivation:
    to_date:
      source: RFSTDTM
```

When the hours matter, the study needs a finer unit than days, which
this vocabulary does not offer; record that gap instead of rounding
it away.
