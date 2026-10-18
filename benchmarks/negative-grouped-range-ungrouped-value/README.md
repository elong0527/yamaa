# Reject a Treatment Period Found From One Administration's Day

**Goal:** build one record per subject from the subject's exposure records,
with the total dose administered in `TOTDOSE` and the treatment period of the
subject's first dose in `APERIOD`.

**Input:** exposure records, one per administration, carrying the study day
(`EXSTDY`) and the dose given (`EXDOSE`), and the study's treatment periods,
each running from a first (`LO`) to a last (`HI`) study day.

**Variables:**

- `TOTDOSE` would be the sum of the doses of the subject's administrations.
- `APERIOD` would be the period, both ends inclusive, that holds the study day
  of the subject's first dose, and blank when no period does.

**Note:** each subject's record summarizes all of the subject's
administrations, and the period is found from an administration's study day.
A subject with several administrations has several such days, so the record
has no single day to place in a period, and the run is rejected before any
data is read and no artifact is accepted.

## How to fix

Derive the day of the first dose on the subject's record, then find the
period from that value:

```yaml
derivations:
  FIRSTDY: {aggregate: "MIN(EX.EXSTDY)"}
```

```yaml
between: {value: FIRSTDY, lower: LO, upper: HI}
```

Leave `FIRSTDY` out of the published columns when the artifact should not
carry it.
