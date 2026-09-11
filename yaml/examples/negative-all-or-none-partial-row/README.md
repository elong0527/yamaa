# ADaM ADSL: reject a row with only some paired dates present

This example uses collected demographics to record one row per subject:

- `RFSTD` is the calendar date treatment started.
- `DTHDT` is the date of death, missing for subjects still followed.

The paired-dates rule requires both dates present together or both
absent. A subject still followed breaks the pairing, so the run is
rejected after the dataset completes and no artifact is accepted.

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
